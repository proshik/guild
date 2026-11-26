//! Durable, at-most-once startup continuation reservations. Never replays a prompt.
use crate::{
    HarnessKind, SessionId,
    store::{Normalise, Store},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Bound argv and socket payloads before any process inspection or reservation.
pub const MAX_PROMPT_BYTES: usize = 32 * 1024;
/// Validate caller-supplied text independently of the transport.
pub fn validate(request_id: &str, prompt: &str) -> Result<(), String> {
    if request_id.is_empty() || request_id.len() > 128 || request_id.contains('\0') {
        return Err("Continuation request_id must contain 1..128 bytes without NUL".into());
    }
    if prompt.trim().is_empty() || prompt.contains('\0') || prompt.len() > MAX_PROMPT_BYTES {
        return Err(
            "Continuation prompt must be nonempty UTF-8, without NUL, at most 32768 bytes".into(),
        );
    }
    Ok(())
}

/// Immutable request identity. The fingerprint is SHA-256 of the complete launch context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    /// None is the existing external operator namespace, never an invented session.
    pub controller: Option<SessionId>,
    /// Window-owned task identity.
    pub session: SessionId,
    /// Stable caller-selected key.
    pub request_id: String,
    /// Absent in historical receipts; version one retains its exact serialized shape.
    #[serde(default = "legacy_version", skip_serializing_if = "is_legacy")]
    pub version: u8,
    /// Explicit in version two; absent historically means Codex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<HarnessKind>,
    /// Startup argv or positively qualified existing prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<DeliveryPath>,
    /// Exact saved harness conversation.
    pub conversation: String,
    /// Includes prompt, front, project, workspace and cwd; never the prompt itself.
    pub fingerprint: String,
}
fn legacy_version() -> u8 {
    1
}
fn is_legacy(version: &u8) -> bool {
    *version == 1
}
/// An attempt never changes transport when retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryPath {
    /// Literal argv on exact same-conversation launch.
    Startup,
    /// Input to an independently qualified existing Codex runtime.
    Live,
}
impl Attempt {
    /// Historical descriptors were Codex startup only.
    pub fn kind(&self) -> HarnessKind {
        self.harness.unwrap_or(HarnessKind::Codex)
    }
}
/// Delivery knowledge, separate from model execution or readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Persisted before a launch attempt.
    Reserved,
    /// Process creation succeeded, not a model delivery acknowledgement.
    Launched,
    /// Terminal bytes were submitted; not an execution acknowledgement.
    Submitted,
    /// Process creation demonstrably failed; retries still do not launch again.
    FailedToStart,
    /// A prior lifetime or storage error cannot establish delivery. Never retry automatically.
    DeliveryUnknown,
}
/// A durable response that can be looked up after a lost reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    /// Stable request and conversation identity.
    pub request: Attempt,
    /// Current delivery knowledge.
    pub state: State,
    /// A new lifecycle start was witnessed by the exact attempted runtime.
    #[serde(default, skip_serializing_if = "is_false")]
    pub turn_started: bool,
}
fn is_false(value: &bool) -> bool {
    !value
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    receipts: Vec<Receipt>,
}
impl Normalise for Saved {
    fn normalise(&mut self) {}
}
/// Strict receipt storage: unreadable or missing-with-backup data never starts a clean ledger.
pub struct Ledger {
    saved: Saved,
    store: Store<Saved>,
    error: Option<String>,
}
impl Ledger {
    /// Recover metadata only. No invocation or text is retained for replay.
    pub fn load(path: PathBuf) -> Self {
        let mut backup = path.as_os_str().to_owned();
        backup.push(".bak");
        let loaded = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Saved>(&bytes)
                .map_err(|_| "Continuation ledger is unreadable".to_owned()),
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && !std::path::Path::new(&backup).exists() =>
            {
                Ok(Saved::default())
            }
            Err(_) => Err("Continuation ledger could not be read".into()),
        };
        let mut ledger = Self {
            saved: Saved::default(),
            store: Store::new(path),
            error: None,
        };
        match loaded {
            Ok(mut saved) => {
                for (index, receipt) in saved.receipts.iter().enumerate() {
                    let r = &receipt.request;
                    if !valid_attempt(r)
                        || saved.receipts[..index]
                            .iter()
                            .any(|p| same_key(&p.request, r))
                    {
                        ledger.error = Some("Invalid or duplicate continuation receipt".into());
                        return ledger;
                    }
                }
                for receipt in &mut saved.receipts {
                    if receipt.state == State::Reserved {
                        receipt.state = State::DeliveryUnknown;
                    }
                }
                ledger.saved = saved;
            }
            Err(error) => ledger.error = Some(error),
        }
        ledger
    }
    fn healthy(&self) -> Result<(), String> {
        self.error.clone().map_or(Ok(()), Err)
    }
    fn persist(&mut self) -> Result<(), String> {
        self.store.save(&self.saved).map_err(|_| {
            let error =
                "Continuation receipt could not be saved; delivery must not be retried".to_owned();
            self.error = Some(error.clone());
            error
        })
    }
    /// Lookup is scoped identically for external operators and bound session controllers.
    pub fn lookup(
        &self,
        controller: Option<SessionId>,
        session: SessionId,
        request_id: &str,
    ) -> Result<Receipt, String> {
        self.healthy()?;
        self.saved
            .receipts
            .iter()
            .find(|r| {
                r.request.controller == controller
                    && r.request.session == session
                    && r.request.request_id == request_id
            })
            .cloned()
            .ok_or("No continuation receipt for that controller/session/request_id".into())
    }
    /// Strict optional lookup preserves legacy retry descriptors before admission.
    pub fn previous(
        &self,
        controller: Option<SessionId>,
        session: SessionId,
        key: &str,
    ) -> Result<Option<Receipt>, String> {
        self.healthy()?;
        Ok(self
            .saved
            .receipts
            .iter()
            .find(|r| {
                r.request.controller == controller
                    && r.request.session == session
                    && r.request.request_id == key
            })
            .cloned())
    }
    /// Persist observation without upgrading process creation to readiness/completion.
    pub fn mark_started(&mut self, request: &Attempt) -> Result<(), String> {
        self.healthy()?;
        let receipt = self
            .saved
            .receipts
            .iter_mut()
            .find(|r| r.request == *request)
            .ok_or("Continuation reservation missing")?;
        if !matches!(receipt.state, State::Launched | State::Submitted) {
            return Err("Continuation was not launched/submitted".into());
        }
        if receipt.turn_started {
            return Ok(());
        }
        receipt.turn_started = true;
        self.persist()
    }
    /// Compare before admission checks: an identical retry may refer to a now-open task.
    pub fn existing(&self, request: &Attempt) -> Result<Option<Receipt>, String> {
        self.healthy()?;
        if let Some(receipt) = self
            .saved
            .receipts
            .iter()
            .find(|r| same_key(&r.request, request))
        {
            if &receipt.request != request {
                return Err("Continuation request_id already has a different descriptor".into());
            }
            return Ok(Some(receipt.clone()));
        }
        Ok(None)
    }
    /// Uncertain delivery excludes all overlapping keys/controllers, including after restart.
    pub fn unresolved(
        &self,
        session: SessionId,
        kind: HarnessKind,
        conversation: &str,
    ) -> Result<bool, String> {
        self.healthy()?;
        Ok(self.saved.receipts.iter().any(|r| {
            (r.request.session == session
                || (r.request.kind() == kind && r.request.conversation == conversation))
                && (matches!(r.state, State::Reserved | State::DeliveryUnknown)
                    || (matches!(r.state, State::Launched | State::Submitted) && !r.turn_started))
        }))
    }
    /// Reserve and synchronously persist. Only true grants one launch attempt.
    pub fn reserve(&mut self, request: &Attempt) -> Result<(Receipt, bool), String> {
        if let Some(receipt) = self.existing(request)? {
            return Ok((receipt, false));
        }
        if !valid_attempt(request) {
            return Err("Invalid continuation descriptor".into());
        }
        if self.unresolved(request.session, request.kind(), &request.conversation)? {
            return Err("delivery_unknown: another continuation has a reserved or unobserved outcome for this conversation".into());
        }
        let receipt = Receipt {
            request: request.clone(),
            state: State::Reserved,
            turn_started: false,
        };
        self.saved.receipts.push(receipt.clone());
        self.persist()?;
        Ok((receipt, true))
    }
    /// Save the outcome. Storage uncertainty never authorizes another attempt.
    pub fn finish(&mut self, request: &Attempt, launched: bool) -> Result<Receipt, String> {
        self.healthy()?;
        let index = self
            .saved
            .receipts
            .iter()
            .position(|r| &r.request == request)
            .ok_or("No continuation reservation")?;
        if self.saved.receipts[index].state != State::Reserved {
            return Err("Continuation already attempted".into());
        }
        self.saved.receipts[index].state = if launched {
            if request.path == Some(DeliveryPath::Live) {
                State::Submitted
            } else {
                State::Launched
            }
        } else {
            State::FailedToStart
        };
        self.persist()?;
        Ok(self.saved.receipts[index].clone())
    }
}
fn same_key(a: &Attempt, b: &Attempt) -> bool {
    a.controller == b.controller && a.session == b.session && a.request_id == b.request_id
}
fn valid_attempt(r: &Attempt) -> bool {
    let schema = match r.version {
        1 => r.harness.is_none() && r.path.is_none(),
        2 => {
            r.harness.is_some()
                && r.path.is_some()
                && (r.path != Some(DeliveryPath::Live) || r.kind() == HarnessKind::Codex)
        }
        _ => false,
    };
    schema
        && !r.request_id.is_empty()
        && r.request_id.len() <= 128
        && !r.request_id.contains('\0')
        && valid_conversation(r.kind(), &r.conversation)
        && r.fingerprint.len() == 64
        && r.fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
}
/// IDs are never interpreted as flags, titles, latest-session selectors or paths.
pub fn valid_conversation(kind: HarnessKind, conversation: &str) -> bool {
    if kind == HarnessKind::OpenCode {
        conversation.starts_with("ses_")
            && conversation.len() > 4
            && conversation.len() <= 256
            && conversation
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    } else {
        uuid::Uuid::parse_str(conversation).is_ok()
    }
}

/// Preserve the external-operator route; supplied identities must be known, open and non-task.
pub fn authorize_controller(
    desk: &crate::Desk,
    controller: Option<SessionId>,
) -> Result<(), String> {
    if let Some(id) = controller {
        let record = desk
            .session(id)
            .ok_or("Calling session is not in this window")?;
        if !record.open || record.task {
            return Err(
                "Continuation controller must be an open non-task session or external operator"
                    .into(),
            );
        }
    }
    Ok(())
}
/// Startup handoff is never permission to type into any existing live or exited tab.
pub fn admit_target(record: &crate::SessionRecord, in_tab: bool) -> Result<String, String> {
    if !record.task
        || !matches!(
            record.runs,
            crate::Runs::Claude(crate::SessionMode::Terminal)
                | crate::Runs::Codex(crate::SessionMode::Terminal)
                | crate::Runs::OpenCode(crate::SessionMode::Terminal)
        )
    {
        return Err("Continuation resume supports only window-owned terminal tasks".into());
    }
    if !record.cwd.is_absolute() {
        return Err("Task working directory is not absolute".into());
    }
    if record.open || in_tab {
        return Err(
            "Session has an existing runtime; reconcile its ownership/readiness before continuation resume".into(),
        );
    }
    target_conversation(record)
}
/// Validate immutable task provenance even when reading a receipt for an open retry.
pub fn target_conversation(record: &crate::SessionRecord) -> Result<String, String> {
    if !record.task
        || !matches!(
            record.runs,
            crate::Runs::Claude(crate::SessionMode::Terminal)
                | crate::Runs::Codex(crate::SessionMode::Terminal)
                | crate::Runs::OpenCode(crate::SessionMode::Terminal)
        )
    {
        return Err("Continuation resume supports only window-owned terminal tasks".into());
    }
    let claude_id = record.id.to_string();
    if record.runs.harness() == Some(HarnessKind::Claude)
        && record
            .harness_session
            .as_deref()
            .is_some_and(|id| id != claude_id)
    {
        return Err(
            "binding_changed: saved Claude conversation differs from its assigned task UUID".into(),
        );
    }

    let conversation = record
        .harness_session
        .as_deref()
        .or_else(|| {
            (record.runs.harness() == Some(HarnessKind::Claude)).then_some(claude_id.as_str())
        })
        .ok_or("Task has no saved harness conversation")?;
    if !valid_conversation(
        record.runs.harness().ok_or("Task harness missing")?,
        conversation,
    ) {
        return Err("Task has an invalid saved harness conversation".into());
    }
    Ok(conversation.into())
}

/// Stable identity for continuation receipts, including receipts from older Guild builds.
/// Keep the serialized JSON bytes and lowercase SHA-256 identical across upgrades.
pub fn fingerprint(value: &impl serde::Serialize) -> Result<String, serde_json::Error> {
    use sha2::{Digest, Sha256};
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

/// Read-only preflight. A successful query does not grant permission to send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    /// Window-owned task ID.
    pub session: SessionId,
    /// Original harness; never a fallback provider.
    pub harness: HarnessKind,
    /// Exact saved conversation, when established.
    pub conversation: String,
    /// Existing lifecycle status, including unknown.
    pub status: String,
    /// live, stopped or unknown; closed records alone are not proof of exit.
    pub runtime: String,
    /// ready, blocked or unqualified.
    pub readiness: String,
    /// Concrete missing evidence or supported delivery path.
    pub reason: String,
}
