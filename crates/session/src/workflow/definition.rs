//! Versioned data selects capabilities; it cannot register executable actions.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A strict, portable version-one workflow graph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    /// Wire schema version, currently one.
    pub version: u32,
    /// Project definition identity.
    pub id: String,
    /// Human-readable process name.
    pub label: String,
    /// Named execution roles.
    pub roles: BTreeMap<String, Role>,
    /// Checked outcomes referenced by steps.
    pub outcomes: BTreeMap<String, Outcome>,
    /// Dependency graph. Array order is the stable tie breaker.
    pub steps: Vec<Step>,
    /// External instruction references; loading never installs skills.
    pub skills: Vec<SkillReference>,
    /// Explicit bounded backwards edges; ordinary dependencies must be acyclic.
    pub corrections: Vec<Correction>,
    /// Aggregate limits preserved across steps, attempts and recovery.
    pub limits: Limits,
}

/// The authority class of a role. A payload cannot grant this authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Owner {
    /// The actual initiating parent, checked by the host.
    Parent,
    /// An assigned visible child, forbidden from independent review/delegation.
    Managed,
    /// External independent reviewer operated by the parent.
    Review,
}

/// Role settings, frozen with each run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    /// Display label, not an authority token.
    pub label: String,
    /// Host-verified authority class.
    pub owner: Owner,
    /// Explicit harness selection; absence is resolved before execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<crate::HarnessKind>,
    /// Explicit provider model, never inferred from parent harness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Provider-specific effort selected by the person/configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

/// Native validator vocabulary. Configuration cannot add validators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Validator {
    /// Contract artifacts and actual validation output.
    Contract,
    /// Visible managed-session receipt, source and retained report.
    ManagedSession,
    /// Installed external revust archive and actual-attempt accounting.
    ExternalReview,
    /// Recorded command and required live observations.
    Observation,
    /// Prepared local branch and concrete MR text.
    LocalDelivery,
}

/// Evidence requirement referenced by a step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    /// One supported mechanical evidence validator.
    pub validator: Validator,
    /// Required outcomes gate final readiness.
    pub required: bool,
}

/// Finite host capability vocabulary; no shell/program text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Select and prepare the sole target contract.
    #[serde(rename = "contract.prepare")]
    ContractPrepare,
    /// Assign a visible managed session through guarded native actions.
    #[serde(rename = "session.assign")]
    SessionAssign,
    /// Request parent-owned external review.
    #[serde(rename = "review.request")]
    ReviewRequest,
    /// Collect source-qualified live acceptance.
    #[serde(rename = "acceptance.collect")]
    AcceptanceCollect,
    /// Prepare local delivery artifacts.
    #[serde(rename = "delivery.prepare")]
    DeliveryPrepare,
    /// Evidence-backed parent decision.
    #[serde(rename = "checkpoint.decide")]
    CheckpointDecide,
}

/// Pinned contract route; never changes because a directory later appears.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractKind {
    /// One matching existing OpenSpec change.
    Openspec,
    /// One lightweight note in the target's established documentation.
    Lightweight,
}

/// Typed predicates over observed capability and checked outcome data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    /// Every predicate must hold.
    All {
        /// Predicates evaluated in order.
        conditions: Vec<Condition>,
    },
    /// At least one predicate must hold.
    Any {
        /// Predicates evaluated in order.
        conditions: Vec<Condition>,
    },
    /// Invert one predicate.
    Not {
        /// Nested predicate.
        condition: Box<Condition>,
    },
    /// Exact pinned contract route.
    Contract {
        /// Required contract kind.
        kind: ContractKind,
    },
    /// Explicit observed host capability.
    Capability {
        /// Qualified capability name.
        name: String,
        /// Expected support value.
        supported: bool,
    },
    /// A source-qualified outcome, never an authored 'done' claim.
    Outcome {
        /// Declared outcome identity.
        id: String,
    },
    /// Pinned architecture-only intent.
    ArchitectureOnly {
        /// Expected intent.
        value: bool,
    },
}

/// One node in the configured graph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Unique node identity.
    pub id: String,
    /// Display label.
    pub label: String,
    /// Supported host operation.
    pub action: Action,
    /// Declared role; validated before effects.
    pub role: String,
    /// Predecessor step identities.
    pub depends_on: Vec<String>,
    /// Evidence outcomes produced by this node.
    pub outcomes: Vec<String>,
    /// Optional data predicate, never evaluated source code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
    /// Skills required by this node, by declared qualified reference ID.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
}

/// Where an external skill may be discovered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    /// Prefer project, otherwise a qualified global root.
    Auto,
    /// Project discovery roots only.
    Project,
    /// Qualified global discovery roots only.
    Global,
}

/// External instructions, never an executable plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillReference {
    /// Reference identity used by steps.
    pub id: String,
    /// Skill directory name within discovery root.
    pub name: String,
    /// Explicit discovery scope.
    pub scope: SkillScope,
    /// Missing required instructions block only dependent steps.
    pub required: bool,
}

/// An explicit finite correction edge between submissions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Correction {
    /// Review/check step that can request correction.
    pub from: String,
    /// Earlier step to resubmit in the same eligible session.
    pub to: String,
    /// Finite edge bound, within the aggregate correction ceiling.
    pub max_rounds: u32,
}

/// Per-run aggregate admission ceilings. Zero disables further admission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Every actual provider process, including failed/retry/stage attempts.
    pub review_calls: u32,
    /// Plan and code corrections share this ceiling.
    pub correction_rounds: u32,
}

fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.as_bytes()[0].is_ascii_lowercase()
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c))
}
fn label(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}
fn require(ok: bool, field: impl Into<String>, reason: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("{}: {reason}", field.into()))
    }
}

impl Definition {
    /// Decode and validate before admitting any effects.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let raw = super::json::read(bytes)?;
        if let Some(steps) = raw.get("steps").and_then(serde_json::Value::as_array) {
            for (i, step) in steps.iter().enumerate() {
                if let Some(value) = step.get("action") {
                    serde_json::from_value::<Action>(value.clone())
                        .map_err(|e| format!("steps[{i}].action: {e}"))?;
                }
                if let Some(value) = step.get("condition").filter(|v| !v.is_null()) {
                    serde_json::from_value::<Condition>(value.clone())
                        .map_err(|e| format!("steps[{i}].condition: {e}"))?;
                }
            }
        }
        let definition: Self = serde_json::from_value(raw).map_err(|e| e.to_string())?;
        definition.validate()?;
        Ok(definition)
    }

    /// Embedded default graph; loading it does not launch anything.
    pub fn shipped() -> Result<Self, String> {
        Self::parse(include_bytes!("../../assets/workflows/development.json"))
    }

    /// Merge documented maps recursively and replace arrays/scalars.
    /// Precedence: shipped, project, explicit selection/overrides. Explicit
    /// false and zero survive; unknown keys are rejected after merging.
    pub fn resolve(project: Option<&[u8]>, explicit: Option<&[u8]>) -> Result<Self, String> {
        let definition = Self::layers(project, explicit)?;
        definition.validate()?;
        Ok(definition)
    }

    /// Transport checks data shape without resolving project-owned references.
    /// Only the host can validate the complete shipped -> project -> explicit graph.
    pub(super) fn validate_override(value: &serde_json::Value) -> Result<(), String> {
        Self::layers(
            None,
            Some(&serde_json::to_vec(value).map_err(|e| e.to_string())?),
        )?;
        Ok(())
    }
    fn layers(project: Option<&[u8]>, explicit: Option<&[u8]>) -> Result<Self, String> {
        let mut value: serde_json::Value =
            serde_json::from_slice(include_bytes!("../../assets/workflows/development.json"))
                .map_err(|e| e.to_string())?;
        fn merge(base: &mut serde_json::Value, overlay: serde_json::Value) {
            match (base, overlay) {
                (serde_json::Value::Object(base), serde_json::Value::Object(overlay)) => {
                    for (key, value) in overlay {
                        if let Some(current) = base.get_mut(&key) {
                            merge(current, value);
                        } else {
                            base.insert(key, value);
                        }
                    }
                }
                (base, overlay) => *base = overlay,
            }
        }
        for bytes in [project, explicit].into_iter().flatten() {
            merge(&mut value, super::json::read(bytes)?);
            // Structural errors in an earlier input must not be masked by a
            // later replacement array or value. References are checked only
            // after the complete effective graph has been resolved.
            serde_json::from_value::<Self>(value.clone()).map_err(|e| e.to_string())?;
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }

    /// Check references and authority classes, with actionable field paths.
    pub fn validate(&self) -> Result<(), String> {
        require(self.version == 1, "version", "only version 1 is supported")?;
        require(identifier(&self.id), "id", "expected [a-z][a-z0-9_-]{0,63}")?;
        require(
            label(&self.label),
            "label",
            "expected a nonempty single-line label (<=512 bytes)",
        )?;
        require(
            !self.steps.is_empty() && self.steps.len() <= 256,
            "steps",
            "expected 1..256 steps",
        )?;
        require(
            self.limits.review_calls <= 10000,
            "limits.review_calls",
            "maximum 10000",
        )?;
        require(
            self.limits.correction_rounds <= 100,
            "limits.correction_rounds",
            "maximum 100",
        )?;
        for (id, role) in &self.roles {
            require(identifier(id), format!("roles.{id}"), "invalid role ID")?;
            require(
                label(&role.label),
                format!("roles.{id}.label"),
                "invalid label",
            )?;
            for (field, value) in [("model", &role.model), ("effort", &role.effort)] {
                crate::harness::LaunchSelection::validate_option(field, value.as_deref())
                    .map_err(|reason| format!("roles.{id}.{field}: {reason}"))?;
            }
        }
        for id in self.outcomes.keys() {
            require(
                identifier(id),
                format!("outcomes.{id}"),
                "invalid outcome ID",
            )?;
        }
        let mut skills = BTreeSet::new();
        for (i, skill) in self.skills.iter().enumerate() {
            require(
                identifier(&skill.id) && skills.insert(skill.id.as_str()),
                format!("skills[{i}].id"),
                "invalid or duplicate ID",
            )?;
            require(
                identifier(&skill.name),
                format!("skills[{i}].name"),
                "invalid skill directory name",
            )?;
        }
        let mut ids = BTreeSet::new();
        for (i, step) in self.steps.iter().enumerate() {
            require(
                identifier(&step.id) && ids.insert(step.id.as_str()),
                format!("steps[{i}].id"),
                "invalid or duplicate ID",
            )?;
        }
        let mut producers = BTreeSet::new();
        for (i, step) in self.steps.iter().enumerate() {
            let path = format!("steps[{i}]");
            require(label(&step.label), format!("{path}.label"), "invalid label")?;
            let role = self
                .roles
                .get(&step.role)
                .ok_or_else(|| format!("{path}.role: unavailable role {}", step.role))?;
            let expected = match step.action {
                Action::SessionAssign => Some(Owner::Managed),
                Action::ReviewRequest => Some(Owner::Review),
                Action::AcceptanceCollect | Action::CheckpointDecide | Action::DeliveryPrepare => {
                    Some(Owner::Parent)
                }
                Action::ContractPrepare => None,
            };
            require(
                expected.is_none_or(|owner| role.owner == owner)
                    && (step.action != Action::ContractPrepare || role.owner != Owner::Review),
                format!("{path}.role"),
                "role owner is incompatible with action",
            )?;
            let mut unique = BTreeSet::new();
            for dep in &step.depends_on {
                require(
                    ids.contains(dep.as_str()) && dep != &step.id && unique.insert(dep),
                    format!("{path}.depends_on"),
                    "unknown, self or duplicate dependency",
                )?;
            }
            unique.clear();
            for outcome in &step.outcomes {
                require(
                    self.outcomes.contains_key(outcome) && unique.insert(outcome),
                    format!("{path}.outcomes"),
                    "unknown or duplicate outcome",
                )?;
                require(
                    producers.insert(outcome),
                    format!("{path}.outcomes"),
                    "an outcome has exactly one producer",
                )?;
            }
            let mut unique = BTreeSet::new();
            for skill in &step.skills {
                require(
                    skills.contains(skill.as_str()) && unique.insert(skill),
                    format!("{path}.skills"),
                    "unknown or duplicate skill reference",
                )?;
            }
            if let Some(condition) = &step.condition {
                self.validate_condition(condition, &format!("{path}.condition"), 0)?;
            }
        }
        for (id, outcome) in &self.outcomes {
            require(
                !outcome.required || producers.contains(id),
                format!("outcomes.{id}"),
                "required outcome has no producer",
            )?;
        }
        self.order()?;
        let mut edges = BTreeSet::new();
        for (i, edge) in self.corrections.iter().enumerate() {
            require(
                ids.contains(edge.from.as_str())
                    && ids.contains(edge.to.as_str())
                    && edge.from != edge.to
                    && edges.insert((&edge.from, &edge.to)),
                format!("corrections[{i}]"),
                "unknown, self or duplicate correction edge",
            )?;
            require(
                edge.max_rounds > 0 && edge.max_rounds <= self.limits.correction_rounds,
                format!("corrections[{i}].max_rounds"),
                "must be positive and within aggregate correction limit",
            )?;
        }
        Ok(())
    }

    fn validate_condition(
        &self,
        condition: &Condition,
        path: &str,
        depth: usize,
    ) -> Result<(), String> {
        require(depth < 32, path, "condition nesting exceeds 32")?;
        match condition {
            Condition::All { conditions } | Condition::Any { conditions } => {
                require(
                    !conditions.is_empty() && conditions.len() <= 256,
                    path,
                    "expected 1..256 conditions",
                )?;
                for (i, child) in conditions.iter().enumerate() {
                    self.validate_condition(child, &format!("{path}.conditions[{i}]"), depth + 1)?;
                }
            }
            Condition::Not { condition } => {
                self.validate_condition(condition, &format!("{path}.condition"), depth + 1)?
            }
            Condition::Outcome { id } => {
                require(self.outcomes.contains_key(id), path, "unknown outcome")?
            }
            Condition::Capability { name, .. } => require(
                matches!(
                    name.as_str(),
                    "git-checkout" | "native-origin" | "openspec-executable"
                ),
                path,
                "unknown capability; supported names: git-checkout, native-origin, openspec-executable",
            )?,
            Condition::Contract { .. } | Condition::ArchitectureOnly { .. } => {}
        }
        Ok(())
    }

    /// Stable topological order, independent of hardcoded stage names.
    pub fn order(&self) -> Result<Vec<&Step>, String> {
        let mut ordered = Vec::new();
        let mut done = BTreeSet::new();
        while ordered.len() < self.steps.len() {
            let next = self.steps.iter().find(|step| {
                !done.contains(&step.id) && step.depends_on.iter().all(|dep| done.contains(dep))
            });
            let Some(next) = next else {
                return Err(
                    "steps.depends_on: unbounded dependency cycle or unknown dependency".into(),
                );
            };
            done.insert(next.id.clone());
            ordered.push(next);
        }
        Ok(ordered)
    }
}
