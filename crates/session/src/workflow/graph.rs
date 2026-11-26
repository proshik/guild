//! A configured dependency graph reduced from checked observations, never turn status.
use super::{
    Action, Condition, ContractKind, Definition, Evidence, EvidenceContext, Surface, Validator,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Per-node execution knowledge, independent of behavioral verdicts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeState {
    /// A predecessor has not supplied its required outcome.
    Blocked,
    /// Dependencies and observed predicates permit admission.
    Ready,
    /// An actual effect was reserved/delivered, not completed.
    Active,
    /// An observed prerequisite is unavailable or unknown.
    Waiting,
    /// All declared outcomes have checked evidence.
    Finished,
    /// A configured predicate is false.
    Skipped,
    /// Explicit cancellation stopped further admission.
    Cancelled,
}
/// One derived row, in configured topological order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeSnapshot {
    /// Configured identity.
    pub step: String,
    /// Derived state, never set by a UI phase button.
    pub state: NodeState,
    /// Actual activity or prerequisite diagnostic.
    pub detail: Option<String>,
}
/// Retained checked outcome, including historical evidence after invalidation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualifiedOutcome {
    /// Configured producer node.
    step: String,
    /// Declared outcome identity.
    outcome: String,
    /// Mechanically checked evidence from the actual expected producer.
    evidence: Evidence,
    surface: Surface,
}
impl QualifiedOutcome {
    /// Perform accessible-output checks outside the pure graph reducer.
    /// Context comes from actual host bindings and accepted check requirements.
    pub fn check(
        step: &str,
        outcome: &str,
        evidence: Evidence,
        context: &EvidenceContext<'_>,
    ) -> Result<Self, String> {
        if step != context.step {
            return Err("workflow.evidence: configured step differs".into());
        }
        evidence
            .verify(context)
            .map_err(|e| format!("{:?}: {e}", e.verdict))?;
        Ok(Self {
            step: step.into(),
            outcome: outcome.into(),
            evidence,
            surface: context.surface,
        })
    }
    /// Actual retained evidence, including its producer/source/scenario identities.
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }
    /// Declared outcome this evidence qualifies.
    pub fn outcome(&self) -> &str {
        &self.outcome
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorrectionCycle {
    from: String,
    to: String,
}
/// Portable graph state. Hosts retain original events and reconstruct this snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunGraph {
    definition: Definition,
    architecture_only: bool,
    route: Option<ContractKind>,
    capabilities: BTreeMap<String, Option<bool>>,
    history: Vec<QualifiedOutcome>,
    activity: BTreeMap<String, String>,
    active: BTreeSet<String>,
    gaps: BTreeMap<String, String>,
    source: Option<String>,
    contract: Option<String>,
    cancelled: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    history_floor: BTreeMap<String, usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    corrections: Vec<CorrectionCycle>,
}
impl RunGraph {
    /// Loading a graph performs no external action and grants no execution authority.
    pub fn new(definition: Definition, architecture_only: bool) -> Result<Self, String> {
        definition.validate()?;
        Ok(Self {
            definition,
            architecture_only,
            route: None,
            capabilities: BTreeMap::new(),
            history: Vec::new(),
            activity: BTreeMap::new(),
            active: BTreeSet::new(),
            gaps: BTreeMap::new(),
            source: None,
            contract: None,
            cancelled: false,
            history_floor: BTreeMap::new(),
            corrections: vec![],
        })
    }
    /// Frozen effective definition, not the installed default at inspection time.
    pub fn definition(&self) -> &Definition {
        &self.definition
    }
    /// Qualified contract route observed by the host.
    pub fn contract_route(&mut self, route: ContractKind) {
        self.route = Some(route);
    }
    /// Observed capability support; None is unknown, never assumed false/true.
    pub fn capability(&mut self, name: &str, supported: Option<bool>) {
        self.capabilities.insert(name.into(), supported);
    }
    /// Actual telemetry is visible but cannot qualify an outcome.
    pub fn activity(&mut self, step: &str, detail: &str) -> Result<(), String> {
        self.step(step)?;
        self.activity.insert(step.into(), detail.into());
        Ok(())
    }
    /// A correlated effect reservation, not a completion event.
    pub fn admitted(&mut self, step: &str) -> Result<(), String> {
        if self.state(step)? != NodeState::Ready {
            return Err("workflow.step: not ready for effect admission".into());
        }
        self.active.insert(step.into());
        Ok(())
    }
    /// Release an execution flag only after the host observes and accounts for
    /// that effect's completion. Historical outcomes and gaps are unchanged.
    pub fn completed(&mut self, step: &str) -> Result<(), String> {
        self.step(step)?;
        self.active.remove(step);
        Ok(())
    }
    /// A precise dependent gap; unrelated ready nodes remain inspectable.
    pub fn wait(&mut self, step: &str, reason: &str) -> Result<(), String> {
        self.step(step)?;
        self.gaps.insert(step.into(), reason.into());
        Ok(())
    }
    /// Remove only a prerequisite whose host reconciliation actually succeeded.
    pub fn reconciled(&mut self, step: &str) {
        self.gaps.remove(step);
    }
    /// Reduce a mechanically checked observation; hosts never deserialize this from CLI input.
    pub fn qualify(&mut self, qualified: QualifiedOutcome) -> Result<(), String> {
        let step = qualified.step.as_str();
        let outcome = qualified.outcome.as_str();
        let evidence = &qualified.evidence;
        let configured = self.step(step)?;
        if !configured.outcomes.iter().any(|id| id == outcome) {
            return Err("workflow.outcome: not declared by this step".into());
        }
        let validator = self.definition.outcomes[outcome].validator;
        let matches = match validator {
            Validator::Contract => qualified.surface == Surface::Contract,
            Validator::ManagedSession => qualified.surface == Surface::Managed,
            Validator::ExternalReview => qualified.surface == Surface::Review,
            Validator::Observation => matches!(
                qualified.surface,
                Surface::Command | Surface::Api | Surface::Native
            ),
            Validator::LocalDelivery => qualified.surface == Surface::Delivery,
        };
        if !matches {
            return Err(
                "workflow.outcome: evidence surface does not satisfy configured validator".into(),
            );
        }
        if !matches!(
            self.state(step)?,
            NodeState::Ready | NodeState::Active | NodeState::Finished
        ) {
            return Err("workflow.step: dependencies or conditions are not satisfied".into());
        }
        if let Some(index) = self
            .history
            .iter()
            .position(|q| q.evidence.id == evidence.id)
        {
            if index < self.history_floor.get(step).copied().unwrap_or(0) {
                return Err(
                    "workflow.correction: historical evidence cannot qualify a new submission"
                        .into(),
                );
            }
            if self.history.contains(&qualified) {
                return Ok(());
            }
            return Err("workflow.evidence: identity already used for different facts".into());
        }
        if self
            .contract
            .as_deref()
            .is_some_and(|digest| digest != evidence.contract)
        {
            return Err(
                "workflow.contract: accepted identity changed; reconcile the sole contract first"
                    .into(),
            );
        }
        self.contract = Some(evidence.contract.clone());
        self.source = Some(evidence.source.sha256.clone());
        self.gaps.remove(step);
        self.history.push(qualified);
        Ok(())
    }
    /// Bounded resubmissions already admitted by this graph. The host also
    /// enforces imported corrections and actual review usage in the same ledger.
    pub fn corrections_used(&self) -> u32 {
        self.corrections.len() as u32
    }
    /// Reopen only a configured correction edge and its dependants. A host must
    /// qualify parent authority/result and persist its request before effects.
    /// History survives; its old outcomes cannot finish a later submission.
    pub fn correct(&mut self, from: &str, to: &str) -> Result<(), String> {
        let edge = self
            .definition
            .corrections
            .iter()
            .find(|edge| edge.from == from && edge.to == to)
            .ok_or("workflow.correction: no configured correction edge")?;
        if self.cancelled
            || self.corrections_used() >= self.definition.limits.correction_rounds
            || self
                .corrections
                .iter()
                .filter(|c| c.from == from && c.to == to)
                .count()
                >= edge.max_rounds as usize
        {
            return Err("workflow.correction: aggregate or configured edge limit reached".into());
        }
        if !matches!(
            self.state(from)?,
            NodeState::Active | NodeState::Waiting | NodeState::Finished
        ) {
            return Err(
                "workflow.correction: source node has no admitted or checked result".into(),
            );
        }
        let mut affected = BTreeSet::from([to.to_owned()]);
        loop {
            let count = affected.len();
            let outcomes: BTreeSet<_> = self
                .definition
                .steps
                .iter()
                .filter(|step| affected.contains(&step.id))
                .flat_map(|step| step.outcomes.iter().cloned())
                .collect();
            for step in &self.definition.steps {
                if step.depends_on.iter().any(|id| affected.contains(id))
                    || step
                        .condition
                        .as_ref()
                        .is_some_and(|c| condition_uses(c, &outcomes))
                {
                    affected.insert(step.id.clone());
                }
            }
            if affected.len() == count {
                break;
            }
        }
        let contract_changed = self
            .definition
            .steps
            .iter()
            .any(|step| affected.contains(&step.id) && step.action == Action::ContractPrepare);
        for id in affected {
            self.history_floor.insert(id.clone(), self.history.len());
            self.active.remove(&id);
            self.gaps.remove(&id);
            self.activity.remove(&id);
        }
        if contract_changed {
            self.contract = None;
        }
        self.corrections.push(CorrectionCycle {
            from: from.into(),
            to: to.into(),
        });
        self.activity.insert(
            to.into(),
            format!("Correction requested by {from}; checked resubmission required"),
        );
        Ok(())
    }
    /// Changed source invalidates build-dependent review/acceptance, retaining history.
    /// The consumed contract/design approval still describes the builder's accepted base.
    pub fn invalidate_source(&mut self, source: &str) {
        self.source = Some(source.into());
    }
    /// Cancellation is recorded separately from effect stop outcomes owned by the host.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
    /// All retained outcomes, including those now stale.
    pub fn history(&self) -> &[QualifiedOutcome] {
        &self.history
    }
    /// Derive the graph in stable configured dependency order.
    pub fn nodes(&self) -> Vec<NodeSnapshot> {
        let order = self.definition.order().expect("validated frozen graph");
        let mut states = BTreeMap::new();
        // Only definitive producer outcomes propagate. Cycles without an
        // independently qualified outcome stay unknown, rather than inventing
        // a false outcome from declaration order.
        for _ in 0..=order.len() {
            let next: BTreeMap<_, _> = order
                .iter()
                .map(|step| {
                    let predicate = step
                        .condition
                        .as_ref()
                        .map(|c| self.condition(c, &states))
                        .unwrap_or(Some(true));
                    let state = if self.cancelled {
                        NodeState::Cancelled
                    } else if !step.depends_on.iter().all(|id| {
                        matches!(
                            states.get(id.as_str()),
                            Some(NodeState::Finished | NodeState::Skipped)
                        )
                    }) {
                        NodeState::Blocked
                    } else if predicate == Some(false) {
                        NodeState::Skipped
                    } else if predicate.is_none() || self.gaps.contains_key(&step.id) {
                        NodeState::Waiting
                    } else if step
                        .outcomes
                        .iter()
                        .all(|outcome| self.valid_outcome(&step.id, outcome))
                    {
                        NodeState::Finished
                    } else if self.active.contains(&step.id) {
                        NodeState::Active
                    } else {
                        NodeState::Ready
                    };
                    (step.id.as_str(), state)
                })
                .collect();
            if next == states {
                break;
            }
            states = next;
        }
        order
            .into_iter()
            .map(|step| NodeSnapshot {
                step: step.id.clone(),
                state: states[step.id.as_str()],
                detail: self
                    .gaps
                    .get(&step.id)
                    .or_else(|| self.activity.get(&step.id))
                    .cloned()
                    .or_else(|| {
                        step.condition
                            .as_ref()
                            .is_some_and(|c| self.condition(c, &states).is_none())
                            .then(|| "configured condition awaits an actual observation".into())
                    }),
            })
            .collect()
    }
    /// Readiness requires checked outcomes for every enabled node, never quiet telemetry.
    pub fn ready(&self) -> bool {
        !self.cancelled
            && self
                .nodes()
                .iter()
                .all(|n| matches!(n.state, NodeState::Finished | NodeState::Skipped))
    }
    fn state(&self, step: &str) -> Result<NodeState, String> {
        self.nodes()
            .into_iter()
            .find(|n| n.step == step)
            .map(|n| n.state)
            .ok_or_else(|| format!("workflow.step: unknown {step}"))
    }
    fn step(&self, step: &str) -> Result<&super::Step, String> {
        self.definition
            .steps
            .iter()
            .find(|s| s.id == step)
            .ok_or_else(|| format!("workflow.step: unknown {step}"))
    }
    fn source_dependent(&self, step: &str, seen: &mut BTreeSet<String>) -> bool {
        if !seen.insert(step.into()) {
            return false;
        }
        let configured = self.step(step).expect("validated graph reference");
        configured.action == Action::SessionAssign
            || configured
                .depends_on
                .iter()
                .any(|id| self.source_dependent(id, seen))
            || configured.condition.as_ref().is_some_and(|condition| {
                self.definition.steps.iter().any(|producer| {
                    condition_uses(condition, &producer.outcomes.iter().cloned().collect())
                        && self.source_dependent(&producer.id, seen)
                })
            })
    }
    fn valid_outcome(&self, step: &str, outcome: &str) -> bool {
        let source_dependent = self.source_dependent(step, &mut BTreeSet::new());
        self.history
            .iter()
            .skip(self.history_floor.get(step).copied().unwrap_or(0))
            .any(|q| {
                q.step == step
                    && q.outcome == outcome
                    && self.contract.as_deref() == Some(q.evidence.contract.as_str())
                    && (!source_dependent
                        || self.source.as_deref() == Some(q.evidence.source.sha256.as_str()))
            })
    }
    fn condition(&self, condition: &Condition, states: &BTreeMap<&str, NodeState>) -> Option<bool> {
        match condition {
            Condition::ArchitectureOnly { value } => Some(*value == self.architecture_only),
            Condition::Contract { kind } => self.route.map(|r| r == *kind),
            Condition::Capability { name, supported } => self
                .capabilities
                .get(name)
                .copied()
                .flatten()
                .map(|v| v == *supported),
            Condition::Outcome { id } => self
                .definition
                .steps
                .iter()
                .find(|s| s.outcomes.contains(id))
                .and_then(|s| {
                    if self.valid_outcome(&s.id, id) {
                        Some(true)
                    } else if matches!(
                        states.get(s.id.as_str()),
                        Some(NodeState::Finished | NodeState::Skipped)
                    ) {
                        Some(false)
                    } else {
                        None
                    }
                }),
            Condition::Not { condition } => self.condition(condition, states).map(|v| !v),
            Condition::All { conditions } => {
                let values: Vec<_> = conditions
                    .iter()
                    .map(|c| self.condition(c, states))
                    .collect();
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.contains(&None) {
                    None
                } else {
                    Some(true)
                }
            }
            Condition::Any { conditions } => {
                let values: Vec<_> = conditions
                    .iter()
                    .map(|c| self.condition(c, states))
                    .collect();
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.contains(&None) {
                    None
                } else {
                    Some(false)
                }
            }
        }
    }
}

fn condition_uses(condition: &Condition, outcomes: &BTreeSet<String>) -> bool {
    match condition {
        Condition::Outcome { id } => outcomes.contains(id),
        Condition::Not { condition } => condition_uses(condition, outcomes),
        Condition::All { conditions } | Condition::Any { conditions } => conditions
            .iter()
            .any(|condition| condition_uses(condition, outcomes)),
        _ => false,
    }
}
