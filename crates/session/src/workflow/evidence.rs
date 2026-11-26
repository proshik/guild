//! Source-qualified evidence is separate from process activity and authored claims.
use crate::SessionId;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The exact source layer a manifest entry observes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceLayer {
    /// Index contents, independent of disk bytes.
    Index,
    /// Current tracked disk contents.
    Working,
    /// Nonignored untracked bytes.
    Untracked,
    /// Tracked path absent from disk.
    Removed,
    /// Recursive submodule source identity.
    Submodule,
}
/// One source path in one observed layer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    /// Root-relative path.
    pub path: PathBuf,
    /// Observed layer.
    pub layer: SourceLayer,
    /// Exact byte or nested-manifest SHA-256.
    pub sha256: String,
    /// Unix type/access bits, including executable changes.
    pub mode: u32,
}
/// Exact source identity, never just a branch name or HEAD.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    /// Actual canonical checkout.
    pub target: PathBuf,
    /// Accepted Git baseline; absent only for a positively unversioned target.
    pub base: Option<String>,
    /// Actual Git HEAD; absent for a positively unversioned target.
    pub commit: Option<String>,
    /// Sorted index, disk and untracked manifest.
    pub files: Vec<SourceFile>,
    /// Digest of the complete identity including all layers.
    pub sha256: String,
}
impl SourceIdentity {
    /// Construct a source observation from already captured Git/file facts.
    pub fn new(
        target: PathBuf,
        base: String,
        commit: String,
        files: Vec<SourceFile>,
    ) -> Result<Self, String> {
        Self::from_observations(target, Some(base), Some(commit), files)
    }
    /// Positively unversioned targets still pin every observed file byte.
    pub fn unversioned(target: PathBuf, files: Vec<SourceFile>) -> Result<Self, String> {
        Self::from_observations(target, None, None, files)
    }
    fn from_observations(
        target: PathBuf,
        base: Option<String>,
        commit: Option<String>,
        mut files: Vec<SourceFile>,
    ) -> Result<Self, String> {
        if !target.is_absolute() {
            return Err("source.target: expected actual absolute checkout".into());
        }
        if base.is_some() != commit.is_some() {
            return Err("source: incomplete revision identity".into());
        }
        for revision in [&base, &commit].into_iter().flatten() {
            if !matches!(revision.len(), 40 | 64)
                || !revision.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err("source: expected full Git revision".into());
            }
        }
        if files.len() > 100_000 {
            return Err("source.files: manifest exceeds 100000 entries".into());
        }
        files.sort_by(|a, b| (&a.path, a.layer).cmp(&(&b.path, b.layer)));
        let mut seen = std::collections::BTreeSet::new();
        for file in &files {
            if file.path.as_os_str().is_empty()
                || file.path.is_absolute()
                || file
                    .path
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err(format!("source.files: unsafe path {}", file.path.display()));
            }
            if !seen.insert((&file.path, file.layer)) {
                return Err(format!(
                    "source.files: duplicate path/layer {}",
                    file.path.display()
                ));
            }
            if !sha256(&file.sha256) {
                return Err(format!(
                    "source.files: invalid digest {}",
                    file.path.display()
                ));
            }
            if file.mode > 0o177777 {
                return Err(format!(
                    "source.files: invalid access/type bits {}",
                    file.path.display()
                ));
            }
        }
        let sha256 = super::digest(
            &serde_json::to_vec(&(&target, &base, &commit, &files)).map_err(|e| e.to_string())?,
        );
        Ok(Self {
            target,
            base,
            commit,
            files,
            sha256,
        })
    }
    /// Reject tampered, duplicate or incomplete manifest identities.
    pub fn verify(&self) -> Result<(), String> {
        let expected = Self::from_observations(
            self.target.clone(),
            self.base.clone(),
            self.commit.clone(),
            self.files.clone(),
        )?;
        if &expected != self {
            return Err("source: manifest or digest changed".into());
        }
        Ok(())
    }
}
/// Surface required by a configured check, distinct from generic exit success.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// Contract preparation and installed schema validation.
    Contract,
    /// Native managed session, delivery and retained report.
    Managed,
    /// External independently owned review archive.
    Review,
    /// Actual command invocation and recorded output.
    Command,
    /// Actual native-window gestures and captured output.
    Native,
    /// Actual API request/response observation.
    Api,
    /// Prepared local branch and concrete MR description.
    Delivery,
}
/// Exact observed tool, excluding environment/credentials.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolObservation {
    /// Actual executable/tool identity.
    pub program: String,
    /// Observed version, not an inferred provider/model.
    pub version: String,
    /// Literal non-secret invocation.
    pub argv: Vec<String>,
}
/// A retained accessible output with expected content identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceArtifact {
    /// Absolute path within an explicitly qualified evidence root.
    pub path: PathBuf,
    /// Exact output SHA-256.
    pub sha256: String,
}
/// Actual observation facts. A model-authored claim is inspectable but never proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Observation {
    /// Rejected for completion, retained for diagnostics.
    Claim {
        /// Authored text.
        message: String,
    },
    /// Contract validation output; lightweight uses its recorded note checks.
    Contract {
        /// Actual validation status.
        exit_code: i32,
        /// Pinned installed schema or explicit lightweight route.
        schema: String,
    },
    /// Actual managed assignment receipt, not a merely queued request.
    Managed {
        /// Assigned session.
        session: SessionId,
        /// Actual checkout.
        checkout: PathBuf,
        /// Correlated delivery receipt.
        delivery: String,
        /// Actual retained builder report.
        report: PathBuf,
    },
    /// External review linkage, further qualified by the native revust bridge.
    Review {
        /// Actual task identity.
        task: String,
        /// Actual round identity.
        round: String,
        /// Retained archive directory.
        archive: PathBuf,
    },
    /// Command output, insufficient for an API/native requirement.
    Command {
        /// Actual process exit status.
        exit_code: i32,
    },
    /// Native observation; fixtures remain explicitly fixture evidence externally.
    Native {
        /// Observed isolated process.
        pid: u32,
        /// Actual native window identity.
        window: u32,
        /// Ordered actual gestures.
        gestures: Vec<String>,
        /// Retained screenshot artifacts.
        screenshots: Vec<PathBuf>,
    },
    /// API response observation, with retained request/response artifacts.
    Api {
        /// Actual method and endpoint without credentials.
        operation: String,
        /// Observed success/error status.
        status: u16,
    },
    /// Local delivery only; publication is outside the finite action vocabulary.
    Delivery {
        /// Existing checked local branch.
        branch: String,
        /// Retained concrete description.
        description: PathBuf,
    },
}
/// Behavioral verdict is independent of session activity/runtime state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    /// Required evidence matches actual checked context/result.
    Pass,
    /// Qualified observation contradicts the declared expected result.
    Fail,
    /// Missing, stale, inaccessible or unqualified evidence.
    Unverified,
}
/// A failed qualification retains its distinction from an observed failed check.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct EvidenceError {
    /// FAIL only for a qualified observed mismatch; otherwise UNVERIFIED.
    pub verdict: Verdict,
    /// Actionable diagnostic without output/credential bytes.
    pub reason: String,
}
impl From<String> for EvidenceError {
    fn from(reason: String) -> Self {
        Self {
            verdict: Verdict::Unverified,
            reason,
        }
    }
}
impl From<&str> for EvidenceError {
    fn from(reason: &str) -> Self {
        reason.to_owned().into()
    }
}
impl EvidenceError {
    fn failed(reason: String) -> Self {
        Self {
            verdict: Verdict::Fail,
            reason,
        }
    }
}
/// Evidence envelope, pinned to producer, contract, scenario and exact source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    /// Immutable observation identity.
    pub id: uuid::Uuid,
    /// Host-verified actual producer.
    pub producer: SessionId,
    /// Accepted contract digest.
    pub contract: String,
    /// Exact observed source, including uncommitted layers.
    pub source: SourceIdentity,
    /// Declared scenario.
    pub scenario: String,
    /// Configured step identity.
    pub step: String,
    /// Actual observed tool and version.
    pub tool: ToolObservation,
    /// Observation time, not a signal that execution completed.
    pub observed_at_ms: u64,
    /// Retained outputs checked before transition.
    pub artifacts: Vec<EvidenceArtifact>,
    /// Actual observation kind/facts.
    pub observation: Observation,
}
/// Expected observable result, frozen with the accepted check requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Expectation {
    /// Exit zero or a successful API status, as appropriate to the surface.
    Success,
    /// An explicitly expected CLI refusal or process result.
    ExitCode(i32),
    /// An explicitly expected API status, including negative scenarios.
    HttpStatus(u16),
    /// A source-qualified observation without an exit/status code.
    Observed,
}
/// Trusted expected context supplied by the coordinator, never a payload role.
#[derive(Clone, Copy)]
pub struct EvidenceContext<'a> {
    /// Actual producer authorized for this assignment/check.
    pub producer: SessionId,
    /// Frozen accepted contract identity.
    pub contract: &'a str,
    /// Current source captured by the host.
    pub source: &'a SourceIdentity,
    /// Configured step requiring this observation.
    pub step: &'a str,
    /// Required accepted scenario.
    pub scenario: &'a str,
    /// Required real surface.
    pub surface: Surface,
    /// Required observable result, never an authored completion boolean.
    pub expectation: Expectation,
    /// Qualified output roots, never inferred from supplied paths.
    pub allowed_roots: &'a [PathBuf],
}
impl Evidence {
    /// Validate mechanical facts; this is not independent parent acceptance.
    pub fn verify(&self, context: &EvidenceContext<'_>) -> Result<(), EvidenceError> {
        if self.producer != context.producer {
            return Err("evidence.producer: not the authorized producer".into());
        }
        if self.contract != context.contract || !sha256(&self.contract) {
            return Err("evidence.contract: accepted contract identity differs".into());
        }
        context.source.verify()?;
        self.source.verify()?;
        if &self.source != context.source {
            return Err(
                "evidence.source: source changed; prior evidence remains historical".into(),
            );
        }
        if self.step != context.step || self.scenario != context.scenario {
            return Err("evidence: step/scenario does not match the configured check".into());
        }
        if self.tool.program.trim().is_empty()
            || self.tool.version.trim().is_empty()
            || self.tool.version.eq_ignore_ascii_case("unknown")
            || self.tool.argv.is_empty()
            || self.tool.argv.iter().any(|v| v.contains('\0'))
            || self.observed_at_ms == 0
        {
            return Err("evidence.tool: actual command/version/time are required".into());
        }
        let surface = match &self.observation {
            Observation::Claim { .. } => {
                return Err(
                    "evidence.claim: an authored done claim is not behavioral evidence".into(),
                );
            }
            Observation::Contract { .. } => Surface::Contract,
            Observation::Managed { .. } => Surface::Managed,
            Observation::Review { .. } => Surface::Review,
            Observation::Command { .. } => Surface::Command,
            Observation::Native { .. } => Surface::Native,
            Observation::Api { .. } => Surface::Api,
            Observation::Delivery { .. } => Surface::Delivery,
        };
        if surface != context.surface {
            return Err("evidence.surface: command/static output cannot satisfy a different required live surface".into());
        }
        if self.artifacts.is_empty() {
            return Err("evidence.artifacts: retained accessible output is required".into());
        }
        let mut paths = std::collections::BTreeSet::new();
        for artifact in &self.artifacts {
            if !artifact.path.is_absolute()
                || !paths.insert(&artifact.path)
                || !sha256(&artifact.sha256)
            {
                return Err("evidence.artifact: invalid or duplicate output identity".into());
            }
            let path = artifact
                .path
                .canonicalize()
                .map_err(|e| format!("evidence.artifact {}: {e}", artifact.path.display()))?;
            let allowed = context
                .allowed_roots
                .iter()
                .filter_map(|r| r.canonicalize().ok())
                .any(|r| path.starts_with(r));
            if !allowed {
                return Err(format!(
                    "evidence.artifact {}: outside qualified output roots",
                    artifact.path.display()
                )
                .into());
            }
            if super::digest(&super::contract::bounded_read(&path)?) != artifact.sha256 {
                return Err(format!(
                    "evidence.artifact {}: output bytes changed",
                    artifact.path.display()
                )
                .into());
            }
        }
        let includes = |path: &Path| self.artifacts.iter().any(|a| a.path == path);
        match &self.observation {
            Observation::Command { exit_code } | Observation::Contract { exit_code, .. } => {
                let expected = match context.expectation {
                    Expectation::Success => 0,
                    Expectation::ExitCode(value) => value,
                    _ => {
                        return Err("evidence: command requires a declared exit expectation".into());
                    }
                };
                if *exit_code != expected {
                    return Err(EvidenceError::failed(format!(
                        "evidence: exit {exit_code}, expected {expected}"
                    )));
                }
                if let Observation::Contract { schema, .. } = &self.observation
                    && schema.trim().is_empty()
                {
                    return Err(
                        "evidence.contract: pinned schema/lightweight validation identity required"
                            .into(),
                    );
                }
            }
            Observation::Managed {
                session,
                checkout,
                delivery,
                report,
            } => {
                if *session != context.producer
                    || checkout != &self.source.target
                    || delivery.trim().is_empty()
                    || !includes(report)
                {
                    return Err(
                        "evidence.managed: actual assignment/delivery/source report required"
                            .into(),
                    );
                }
            }
            Observation::Native {
                pid,
                window,
                gestures,
                screenshots,
            } => {
                if *pid == 0
                    || *window == 0
                    || gestures.is_empty()
                    || gestures.iter().any(|s| s.trim().is_empty())
                    || screenshots.is_empty()
                    || screenshots.iter().any(|p| !includes(p))
                {
                    return Err("evidence.native: actual isolated process/window/gestures/screenshots required".into());
                }
                for screenshot in screenshots {
                    if !super::contract::bounded_read(screenshot)?.starts_with(b"\x89PNG\r\n\x1a\n")
                    {
                        return Err(
                            "evidence.native: screenshot must be a retained PNG capture".into()
                        );
                    }
                }
            }
            Observation::Api { operation, status } => {
                let matches = match context.expectation {
                    Expectation::Success => (200..300).contains(status),
                    Expectation::HttpStatus(expected) => *status == expected,
                    _ => false,
                };
                if operation.trim().is_empty() || !(100..600).contains(status) {
                    return Err(
                        "evidence.api: actual operation and valid status are required".into(),
                    );
                }
                if !matches {
                    return Err(EvidenceError::failed(
                        "evidence.api: actual status does not match declared expectation".into(),
                    ));
                }
            }
            Observation::Delivery {
                branch,
                description,
            } => {
                if branch.trim().is_empty()
                    || branch.starts_with('-')
                    || !includes(description)
                    || super::contract::bounded_read(description)?.is_empty()
                {
                    return Err("evidence.delivery: local branch and concrete retained description required".into());
                }
            }
            Observation::Review {
                task,
                round,
                archive,
            } => {
                if task.trim().is_empty() || round.trim().is_empty() {
                    return Err("evidence.review: task and round identity required".into());
                }
                for name in [
                    "manifest.json",
                    "findings.json",
                    "report.md",
                    "revust/events.jsonl",
                ] {
                    if !includes(&archive.join(name)) {
                        return Err(format!(
                            "evidence.review: qualified archive artifact {name} required"
                        )
                        .into());
                    }
                }
                let manifest: serde_json::Value = serde_json::from_slice(
                    &super::contract::bounded_read(&archive.join("manifest.json"))?,
                )
                .map_err(|e| e.to_string())?;
                if manifest["task"] != *task
                    || manifest["run"] != *round
                    || manifest["outcome"]["state"] != "complete"
                    || manifest["finished_at"].as_str().is_none_or(str::is_empty)
                    // The installed SDK serializes an empty manifest set as
                    // null. Actual report source completeness is still required.
                    || !manifest["degraded"].is_null() && manifest["degraded"]
                        .as_array()
                        .is_none_or(|v| !v.is_empty())
                {
                    return Err(
                        "evidence.review: incomplete, degraded or different archive identity"
                            .into(),
                    );
                }
                let findings: serde_json::Value = serde_json::from_slice(
                    &super::contract::bounded_read(&archive.join("findings.json"))?,
                )
                .map_err(|e| e.to_string())?;
                if findings["outcome"] != manifest["outcome"]
                    || findings["sources"]["expected"]
                        .as_u64()
                        .is_none_or(|v| v == 0)
                    || findings["sources"]["reported"] != findings["sources"]["expected"]
                    || findings["sources"]["degraded"]
                        .as_array()
                        .is_none_or(|v| !v.is_empty())
                    || findings["findings"]
                        .as_array()
                        .is_none_or(|v| !v.is_empty())
                {
                    return Err(
                        "evidence.review: findings/incomplete evidence cannot be clean PASS".into(),
                    );
                }
                // Bridge admission additionally reconciles actual attempts,
                // policy/source identities and reserved aggregate allowance.
            }
            Observation::Claim { .. } => unreachable!("claims refused before surface checks"),
        }
        Ok(())
    }
}

fn sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
