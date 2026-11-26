//! Version-one workflow control vocabulary shared by CLI and the owning window.
use super::{AttemptId, Definition, EventReceipt, Evidence, Expectation, RunId, Surface};
use crate::SessionId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One observable requirement; explicit initiation checks survive final contract preparation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRequirement {
    /// Configured step and outcome.
    pub step: String,
    /// Declared required outcome.
    pub outcome: String,
    /// Accepted scenario identity.
    pub scenario: String,
    /// Actual surface required by the contract.
    pub surface: Surface,
    /// Expected observable result, including negative cases.
    pub expectation: Expectation,
}
/// Explicit initiation intent; origin authority comes from window records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartRequest {
    /// Protocol version, exactly one.
    pub version: u32,
    /// Stable key for lost-reply retries.
    pub request_id: String,
    /// A new run begins at zero.
    pub expected_revision: u64,
    /// Actual initiating session, checked by Root.
    pub origin: SessionId,
    /// Actual target checkout, independent of the caller's cwd.
    pub target: PathBuf,
    /// Sole OpenSpec change or lightweight note, relative to target.
    pub selection: Option<PathBuf>,
    /// Optional project definition under .guild/workflows/.
    pub definition: Option<PathBuf>,
    /// Explicit last-precedence data override, never executable code.
    pub overrides: Option<serde_json::Value>,
    /// Complete visible development assignment.
    pub task: String,
    /// Design-only intent, frozen for graph predicates.
    #[serde(default)]
    pub architecture_only: bool,
    /// Accepted observable requirements; missing coverage never implies PASS.
    #[serde(default)]
    pub checks: Vec<CheckRequirement>,
}
impl StartRequest {
    /// Validate transport constraints before host IO or reservations.
    pub fn validate(&self) -> Result<(), String> {
        version(self.version)?;
        super::Mutation::new(
            &self.request_id,
            self.expected_revision,
            serde_json::json!({"start":self}),
        )?;
        if self.expected_revision != 0 {
            return Err("workflow.expected_revision: initiation requires zero".into());
        }
        if !self.target.is_absolute() {
            return Err("workflow.target: absolute checkout required".into());
        }
        if self.task.trim().is_empty() || self.task.len() > 128 * 1024 || self.task.contains('\0') {
            return Err("workflow.task: nonempty assignment of at most 128 KiB required".into());
        }
        if let Some(path) = &self.definition
            && (path.is_absolute()
                || !path.starts_with(".guild/workflows")
                || path
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_))))
        {
            return Err("workflow.definition: expected contained .guild/workflows path".into());
        }
        if let Some(value) = &self.overrides {
            Definition::validate_override(value)?;
        }
        if self.checks.len() > 4096
            || self
                .checks
                .iter()
                .any(|c| c.step.is_empty() || c.outcome.is_empty() || c.scenario.is_empty())
        {
            return Err("workflow.checks: bounded named requirements required".into());
        }
        Ok(())
    }
}
/// Source-qualified outcome submission; it never sets a phase or completion boolean.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointRequest {
    /// Protocol version.
    pub version: u32,
    /// Stable retry key.
    pub request_id: String,
    /// Compare-and-set current revision.
    pub expected_revision: u64,
    /// Configured step being observed.
    pub step: String,
    /// Declared outcome being supplied.
    pub outcome: String,
    /// Correlated actual observations, not a model-done claim.
    pub evidence: Vec<Evidence>,
    /// Original-parent final requirements, only with contract preparation.
    /// Omission preserves legacy request identities and existing accepted checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<Vec<CheckRequirement>>,
}
/// Parent-owned retry of a positively failed launch or source-qualified review correction.
/// A launch retry retains its successful checkout. A completed external-review
/// findings receipt reserves correction to the same eligible assigned session;
/// neither operation replays an uncertain delivery or starts a new review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryRequest {
    /// Protocol version.
    pub version: u32,
    /// Full-payload retry identity.
    pub request_id: String,
    /// Current compare-and-set revision.
    pub expected_revision: u64,
    /// Exact failed native launch receipt, or completed external-review reservation
    /// with qualified findings; never an inferred process or archive identity.
    pub attempt: super::AttemptId,
}
/// An existing independently verifiable parent usage receipt. Scope identities
/// preserve provenance; importing usage alone never accepts its review outcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewImport {
    /// Stable unique linked round key.
    pub id: String,
    /// Existing absolute archive, read only.
    pub archive: PathBuf,
    /// Recorded source identity, possibly stale for today's source.
    pub source: String,
    /// Recorded accepted contract identity.
    pub contract: String,
    /// Whether this existing round consumed a correction.
    pub correction: bool,
}
/// An explicit parent request for one ready configured external-review node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRequest {
    /// Protocol version.
    pub version: u32,
    /// Stable full-payload retry identity.
    pub request_id: String,
    /// Compare-and-set current revision.
    pub expected_revision: u64,
    /// Configured review.request node.
    pub step: String,
    /// Explicit selected installed package route.
    pub package: PathBuf,
    /// Optional user configuration root; original files are never changed.
    pub config_dir: Option<PathBuf>,
    /// Explicit profile, otherwise the actual effective configured default.
    pub profile: Option<String>,
    /// Literal complete scope and boundaries.
    pub scope: String,
    /// Maximum actual processes reserved for this round, within the run ceiling.
    pub allowance: u32,
    /// Explicit per-run presentation override.
    #[serde(default)]
    pub keep_open: bool,
    /// Prior linked actual processes enter the same aggregate ledger.
    #[serde(default)]
    pub prior: Vec<ReviewImport>,
}
/// A bounded parent mutation without a forged role field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlRequest {
    /// Protocol version.
    pub version: u32,
    /// Stable retry key.
    pub request_id: String,
    /// Compare-and-set current revision.
    pub expected_revision: u64,
}
/// Explicit original-parent acknowledgement of a current recovery assessment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconcileRequest {
    /// Protocol version.
    pub version: u32,
    /// Full-context idempotency key.
    pub request_id: String,
    /// Current run compare-and-set revision.
    pub expected_revision: u64,
    /// None inspects only; Some acknowledges the exact returned SHA256.
    pub assessment: Option<String>,
}
/// No operation can select another transport or invoke arbitrary executable code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkflowCommand {
    /// Discover this window's supported protocol/adapters.
    Capabilities {
        /// Protocol version.
        version: u32,
    },
    /// Start an explicitly authorized run.
    Start {
        /// Complete frozen intent.
        spec: StartRequest,
    },
    /// Read retained history without launching or resuming.
    Status {
        /// Protocol version.
        version: u32,
        /// Stable process identity.
        run: RunId,
    },
    /// Submit checked evidence for an assigned outcome.
    Checkpoint {
        /// Stable process identity.
        run: RunId,
        /// Qualified request.
        spec: CheckpointRequest,
    },
    /// Request the installed external engine for a ready parent-owned review.
    Review {
        /// Stable process identity.
        run: RunId,
        /// Exact scope/selection and bounded allowance.
        spec: ReviewRequest,
    },
    /// Retry a positively refused native launch, or request same-session correction
    /// from a completed findings review. Review recheck remains a separate parent request.
    Retry {
        /// Stable run identity.
        run: RunId,
        /// Explicit partial-attempt selection.
        spec: RetryRequest,
    },
    /// Reveal the Process view without advancing execution.
    Show {
        /// Protocol version.
        version: u32,
        /// Stable process identity.
        run: RunId,
    },
    /// Reconcile the original conversation; never a new-session fallback.
    Recover {
        /// Stable process identity.
        run: RunId,
        /// Durable control request.
        spec: ControlRequest,
    },
    /// Inspect or explicitly acknowledge recovery facts; never inject a prompt.
    Reconcile {
        /// Stable process identity.
        run: RunId,
        /// Original-parent acknowledgement; navigation grants no authority.
        spec: ReconcileRequest,
    },
    /// Stop explicitly owned effects and retain checkouts/history.
    Cancel {
        /// Stable process identity.
        run: RunId,
        /// Durable control request.
        spec: ControlRequest,
    },
}
impl WorkflowCommand {
    /// Validate versions, retry keys and bounds before reaching the queue.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Capabilities { version: v }
            | Self::Status { version: v, .. }
            | Self::Show { version: v, .. } => version(*v),
            Self::Start { spec } => spec.validate(),
            Self::Checkpoint { spec, .. } => {
                version(spec.version)?;
                super::Mutation::new(
                    &spec.request_id,
                    spec.expected_revision,
                    serde_json::json!({"checkpoint":spec}),
                )?;
                if spec.step.is_empty()
                    || spec.outcome.is_empty()
                    || spec.evidence.is_empty()
                    || spec.evidence.len() > 4096
                {
                    return Err(
                        "workflow.checkpoint: named step/outcome and bounded evidence required"
                            .into(),
                    );
                }
                if spec.checks.as_ref().is_some_and(|checks| {
                    checks.len() > 4096
                        || checks.iter().any(|c| {
                            c.step.is_empty() || c.outcome.is_empty() || c.scenario.is_empty()
                        })
                }) {
                    return Err("workflow.checks: bounded named requirements required".into());
                }
                Ok(())
            }
            Self::Retry { spec, .. } => {
                version(spec.version)?;
                super::Mutation::new(
                    &spec.request_id,
                    spec.expected_revision,
                    serde_json::json!({"retry":spec}),
                )?;
                Ok(())
            }
            Self::Review { spec, .. } => {
                version(spec.version)?;
                super::Mutation::new(
                    &spec.request_id,
                    spec.expected_revision,
                    serde_json::json!({"review":spec}),
                )?;
                if spec.step.is_empty()
                    || !spec.package.is_absolute()
                    || spec.config_dir.as_ref().is_some_and(|p| !p.is_absolute())
                    || spec.scope.trim().is_empty()
                    || spec.scope.len() > 128 * 1024
                    || spec.scope.contains('\0')
                    || spec.allowance == 0
                    || spec.allowance > 10000
                    || spec.prior.len() > 128
                    || spec.prior.iter().any(|p| !p.archive.is_absolute())
                {
                    return Err("workflow.review: named step, absolute selected roots, bounded literal scope and positive allowance required".into());
                }
                Ok(())
            }
            Self::Reconcile { spec, .. } => {
                version(spec.version)?;
                super::Mutation::new(
                    &spec.request_id,
                    spec.expected_revision,
                    serde_json::json!({"reconcile":spec}),
                )?;
                if spec.assessment.as_ref().is_some_and(|h| {
                    h.len() != 64
                        || !h
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                }) {
                    return Err("workflow.reconcile: lowercase SHA256 assessment required".into());
                }
                Ok(())
            }
            Self::Recover { spec, .. } | Self::Cancel { spec, .. } => {
                version(spec.version)?;
                super::Mutation::new(
                    &spec.request_id,
                    spec.expected_revision,
                    serde_json::json!({"control":spec}),
                )?;
                Ok(())
            }
        }
    }
    /// Stable target, absent only for discovery/initiation.
    pub fn run(&self) -> Option<RunId> {
        match self {
            Self::Capabilities { .. } | Self::Start { .. } => None,
            Self::Status { run, .. }
            | Self::Checkpoint { run, .. }
            | Self::Review { run, .. }
            | Self::Retry { run, .. }
            | Self::Show { run, .. }
            | Self::Reconcile { run, .. }
            | Self::Recover { run, .. }
            | Self::Cancel { run, .. } => Some(*run),
        }
    }
}
fn version(value: u32) -> Result<(), String> {
    if value == 1 {
        Ok(())
    } else {
        Err(format!(
            "workflow.version: unsupported {value}, supported 1"
        ))
    }
}
/// Typed transport response. A snapshot is retained facts, never a review verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkflowReply {
    /// Actual host protocol and operation support.
    Capabilities {
        /// Protocol version.
        version: u32,
        /// Supported operation names.
        operations: Vec<String>,
        /// Early parent preparation and current read-only checkpoint context.
        /// Older hosts omit this field and do not qualify this entry route.
        #[serde(default)]
        entry_before_design: bool,
        /// Final ContractPrepare can freeze requirements chosen during design.
        #[serde(default)]
        finalize_checks: bool,
    },
    /// Original idempotent receipt and current retained history.
    Receipt {
        /// Stable run identity.
        run: RunId,
        /// Current revision.
        revision: u64,
        /// Original event receipt.
        receipt: Box<EventReceipt>,
        /// Correlated effect attempt.
        attempt: AttemptId,
        /// Configured graph and observed facts.
        snapshot: serde_json::Value,
    },
    /// Read-only current graph and observed facts.
    Snapshot {
        /// Stable run identity.
        run: RunId,
        /// Current revision.
        revision: u64,
        /// Configured graph and observed facts.
        snapshot: serde_json::Value,
    },
    /// Named missing prerequisite, conflict or refusal.
    Gap {
        /// Stable machine-readable code.
        code: String,
        /// Actionable user-facing diagnostic.
        message: String,
        /// Known run if available.
        run: Option<RunId>,
        /// Known revision if available.
        revision: Option<u64>,
    },
}

/// Strict bounded JSON for CLI payloads, retaining duplicate-key refusal before overlays.
pub fn parse_workflow_input(bytes: &[u8]) -> Result<serde_json::Value, String> {
    super::json::read(bytes)
}

/// Window-owned origin identity, captured independently of request payload roles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginBinding {
    /// Original conversation's stable session identity.
    pub session: SessionId,
    /// Original seat, never the currently selected unrelated editor.
    pub project: crate::ProjectId,
    /// Original workspace.
    pub workspace: crate::WorkspaceId,
    /// Recorded valid origin area.
    pub area: Option<crate::AreaId>,
    /// Actual original harness.
    pub harness: crate::HarnessKind,
    /// Actual saved harness conversation if known.
    pub conversation: Option<String>,
    /// Actual original working directory.
    pub cwd: PathBuf,
}
