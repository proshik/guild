//! Aggregate external-review accounting. No providers, profiles or UI live here.
use super::{Limits, RunId, digest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderAttempt {
    stage: String,
    unit: String,
    number: u64,
}
impl ProviderAttempt {
    fn read(value: &serde_json::Value) -> Result<Self, String> {
        let attempt: Self =
            serde_json::from_value(value.clone()).map_err(|e| format!("review.attempt: {e}"))?;
        if !matches!(attempt.stage.as_str(), "find" | "synthesis" | "verify")
            || attempt.unit.is_empty()
            || attempt.unit.len() > 256
            || attempt.unit.chars().any(char::is_control)
            || attempt.number > 10000
        {
            return Err(
                "review.attempt: unsupported stage or invalid unit/attempt identity".into(),
            );
        }
        Ok(attempt)
    }
}
/// Checked process-attempt facts from the installed engine's durable event format.
/// Hosts still bind the archive/task/job/source; this parser is not a review verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewUsage {
    started: BTreeSet<ProviderAttempt>,
    finished: BTreeSet<ProviderAttempt>,
    closed: bool,
    sha256: String,
}
impl ReviewUsage {
    /// Count actual start events across find, retries, synthesis and verification.
    /// A terminal record must account for every started/finished attempt exactly.
    pub fn from_events(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(
                "review.usage: event stream exceeds64MiB; reservation remains charged".into(),
            );
        }
        let mut usage = Self {
            started: BTreeSet::new(),
            finished: BTreeSet::new(),
            closed: false,
            sha256: digest(bytes),
        };
        for line in bytes
            .split(|b| *b == b'\n')
            .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        {
            if usage.closed {
                return Err("review.usage: events after terminal; usage ambiguous".into());
            }
            let value = super::json::read(line)?;
            match value["type"].as_str() {
                Some("started") => {
                    let id = ProviderAttempt::read(&value["attempt"])?;
                    if !usage.started.insert(id) || usage.started.len() > 10000 {
                        return Err("review.usage: duplicate or excessive process starts".into());
                    }
                }
                Some("finished") => {
                    let id = ProviderAttempt::read(&value["attempt"])?;
                    if !usage.started.contains(&id)
                        || !usage.finished.insert(id)
                        || !value["result"].is_object()
                    {
                        return Err(
                            "review.usage: finished attempt has no unique actual start/result"
                                .into(),
                        );
                    }
                }
                Some("terminal") => {
                    let report = &value["report"];
                    let units = report["units"]
                        .as_array()
                        .ok_or("review.usage: terminal attempt coverage missing")?;
                    if report["state"].is_null() {
                        return Err("review.usage: durable terminal state missing".into());
                    }
                    let mut terminal = BTreeSet::new();
                    for unit in units {
                        for attempt in unit["attempts"]
                            .as_array()
                            .ok_or("review.usage: terminal unit attempts missing")?
                        {
                            if !terminal.insert(ProviderAttempt::read(&attempt["id"])?) {
                                return Err("review.usage: duplicate terminal attempt".into());
                            }
                        }
                    }
                    if terminal != usage.started || usage.finished != usage.started {
                        return Err("review.usage: terminal/start/finish identities disagree; usage uncertain".into());
                    }
                    usage.closed = true;
                }
                _ => {
                    return Err(
                        "review.usage: unsupported event kind; reservation remains charged".into(),
                    );
                }
            }
        }
        Ok(usage)
    }
    /// Known process starts, including unsuccessful/invalid/retried attempts.
    pub fn calls(&self) -> u32 {
        self.started.len() as u32
    }
    /// Complete accounting, independent of whether review succeeded or had findings.
    pub fn closed(&self) -> bool {
        self.closed
    }
    fn validate(&self) -> Result<(), String> {
        if self.started.len() > 10000
            || !self.finished.is_subset(&self.started)
            || self.closed && self.started != self.finished
            || !sha(&self.sha256)
        {
            return Err("review.usage: invalid saved attempt coverage".into());
        }
        for id in &self.started {
            ProviderAttempt::read(&serde_json::to_value(id).map_err(|e| e.to_string())?)?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Round {
    id: String,
    source: String,
    contract: String,
    allowance: u32,
    correction: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    correction_claim: Option<String>,
    usage: Option<ReviewUsage>,
    uncertain: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    imported: bool,
}
impl Round {
    fn active(&self) -> bool {
        self.uncertain || self.usage.as_ref().is_none_or(|u| !u.closed)
    }
    fn used(&self) -> u32 {
        self.usage.as_ref().map_or(0, ReviewUsage::calls)
    }
}
/// One run's aggregate ledger, reconstructed/persisted by its sole coordinator.
/// No independent task ID, correction or restored view replenishes the ceiling.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewLedger {
    version: u32,
    run: RunId,
    limits: Limits,
    rounds: Vec<Round>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    correction_claims: BTreeSet<String>,
}
fn sha(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
impl ReviewLedger {
    /// Freeze ceilings from the effective definition, including explicit zero.
    pub fn new(run: RunId, limits: Limits) -> Result<Self, String> {
        let ledger = Self {
            version: 1,
            run,
            limits,
            rounds: vec![],
            correction_claims: BTreeSet::new(),
        };
        ledger.validate()?;
        Ok(ledger)
    }
    /// Check saved state before admission; a corrupt budget is never repaired.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.rounds.len() > 10000
            || self.limits.review_calls > 10000
            || self.limits.correction_rounds > 100
        {
            return Err("review.ledger: unsupported version or limits".into());
        }
        if self.correction_claims.len() > 100
            || self.correction_claims.iter().any(|id| !identity(id))
        {
            return Err("review.ledger: invalid corrective-work claims".into());
        }
        let mut claims = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for round in &self.rounds {
            if !identity(&round.id)
                || !ids.insert(&round.id)
                || !sha(&round.source)
                || !sha(&round.contract)
                || !round.imported && round.allowance > self.limits.review_calls
                || round.used() > round.allowance
            {
                return Err("review.ledger: invalid round identity/allowance".into());
            }
            if let Some(claim) = &round.correction_claim
                && (!round.correction
                    || !self.correction_claims.contains(claim)
                    || !claims.insert(claim))
            {
                return Err("review.ledger: invalid or reused correction claim".into());
            }
            if let Some(usage) = &round.usage {
                usage.validate()?;
            }
        }
        // Imported historical facts can exceed today's cap or remain uncertain
        // together. They never authorize execution; native reservations still
        // obey their original aggregate and serial limits.
        if self
            .rounds
            .iter()
            .filter(|r| !r.imported)
            .fold(0u32, |sum, r| {
                sum.saturating_add(if r.active() { r.allowance } else { r.used() })
            })
            > self.limits.review_calls
            || self.correction_claims.len() as u32
                + self
                    .rounds
                    .iter()
                    .filter(|r| !r.imported && r.correction && r.correction_claim.is_none())
                    .count() as u32
                > self.limits.correction_rounds
            || self
                .rounds
                .iter()
                .filter(|r| !r.imported && r.active())
                .count()
                > 1
        {
            return Err("review.ledger: aggregate ceiling or serial admission violated".into());
        }
        Ok(())
    }
    /// Starts accounted for by checked observations, never just successful calls.
    pub fn used(&self) -> u32 {
        self.rounds
            .iter()
            .fold(0u32, |total, round| total.saturating_add(round.used()))
    }
    /// Unspent allowance still charged to live/ambiguous rounds.
    pub fn reserved(&self) -> u32 {
        let reserved = self
            .rounds
            .iter()
            .filter(|r| r.active())
            .fold(0u32, |total, round| {
                total.saturating_add(round.allowance.saturating_sub(round.used()))
            });
        if self.rounds.iter().any(|r| r.imported && r.active()) {
            reserved.max(self.limits.review_calls.saturating_sub(self.used()))
        } else {
            reserved
        }
    }
    /// Remaining aggregate capacity; an ambiguous round still blocks serial admission.
    pub fn remaining(&self) -> u32 {
        self.limits
            .review_calls
            .saturating_sub(self.used().saturating_add(self.reserved()))
    }
    /// Corrections across both plan and code, including uncertain admitted rounds.
    pub fn corrections_used(&self) -> u32 {
        self.correction_claims.len() as u32
            + self
                .rounds
                .iter()
                .filter(|r| r.correction && r.correction_claim.is_none())
                .count() as u32
    }
    /// Charge corrective work before attempting delivery. A failed/unknown send
    /// keeps the same claim; a later linked review cannot charge it twice.
    pub fn claim_correction(&mut self, id: &str) -> Result<bool, String> {
        self.validate()?;
        if !identity(id) {
            return Err("review.ledger: invalid correction claim identity".into());
        }
        if self.correction_claims.contains(id) {
            return Ok(false);
        }
        if self.corrections_used() >= self.limits.correction_rounds {
            return Err("review.ledger: correction ceiling exhausted".into());
        }
        self.correction_claims.insert(id.into());
        Ok(true)
    }
    /// Whether an admitted correction still has no linked review reservation.
    pub fn correction_pending(&self, id: &str) -> bool {
        self.correction_claims.contains(id)
            && !self
                .rounds
                .iter()
                .any(|r| r.correction_claim.as_deref() == Some(id))
    }
    /// Reserve the one review of an already charged corrective-work claim.
    pub fn reserve_for_correction(
        &mut self,
        id: &str,
        source: &str,
        contract: &str,
        allowance: u32,
        claim: &str,
    ) -> Result<bool, String> {
        self.reserve_inner(id, source, contract, allowance, true, Some(claim))
    }
    /// Reserve before the installed launcher may start any provider process.
    /// Same identity/context coalesces; another round cannot race a live one.
    pub fn reserve(
        &mut self,
        id: &str,
        source: &str,
        contract: &str,
        allowance: u32,
        correction: bool,
    ) -> Result<bool, String> {
        self.reserve_inner(id, source, contract, allowance, correction, None)
    }
    fn reserve_inner(
        &mut self,
        id: &str,
        source: &str,
        contract: &str,
        allowance: u32,
        correction: bool,
        claim: Option<&str>,
    ) -> Result<bool, String> {
        self.validate()?;
        if claim.is_some_and(|id| !self.correction_claims.contains(id)) {
            return Err("review.ledger: corrective-work claim is not admitted".into());
        }
        if let Some(old) = self.rounds.iter().find(|r| r.id == id) {
            if old.source != source
                || old.contract != contract
                || old.allowance != allowance
                || old.correction != correction
                || old.correction_claim.as_deref() != claim
            {
                return Err("review.ledger: round key context conflict".into());
            }
            return Ok(false);
        }
        if !identity(id)
            || !sha(source)
            || !sha(contract)
            || allowance == 0
            || allowance > self.remaining()
        {
            return Err("review.ledger: no bounded remaining review allowance".into());
        }
        if self.rounds.iter().any(Round::active) {
            return Err(
                "review.ledger: another round is live or its usage remains uncertain".into(),
            );
        }
        if claim.is_some_and(|id| {
            self.rounds
                .iter()
                .any(|r| r.correction_claim.as_deref() == Some(id))
        }) {
            return Err(
                "review.ledger: corrective-work claim already has a review reservation".into(),
            );
        }
        if correction && claim.is_none() && self.corrections_used() >= self.limits.correction_rounds
        {
            return Err("review.ledger: correction ceiling exhausted".into());
        }
        self.rounds.push(Round {
            id: id.into(),
            source: source.into(),
            contract: contract.into(),
            allowance,
            correction,
            correction_claim: claim.map(str::to_owned),
            usage: None,
            uncertain: false,
            imported: false,
        });
        Ok(true)
    }
    /// Reconcile the same round monotonically. Bad usage keeps its allowance
    /// charged and blocks every subsequent round instead of reverting a budget.
    pub fn observe(&mut self, id: &str, usage: ReviewUsage) -> Result<(), String> {
        let round = self
            .rounds
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("review.ledger: no reserved round")?;
        if let Err(error) = usage.validate() {
            round.uncertain = true;
            return Err(error);
        }
        if usage.calls() > round.allowance
            || round.usage.as_ref().is_some_and(|old| {
                !old.started.is_subset(&usage.started)
                    || !old.finished.is_subset(&usage.finished)
                    || old.closed && old != &usage
            })
        {
            round.uncertain = true;
            return Err("review.ledger: observed usage exceeds or contradicts reservation; reconciliation required".into());
        }
        round.usage = Some(usage);
        round.uncertain = false;
        self.validate()
    }
    /// An interrupted/unreadable observer cannot release outstanding allowance.
    pub fn uncertain(&mut self, id: &str) -> Result<(), String> {
        let round = self
            .rounds
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("review.ledger: no reserved round")?;
        round.uncertain = true;
        Ok(())
    }
    /// Older native observer/correction snapshots cannot erase later imported facts.
    /// Only a new explicit import observation may reconcile those exact rows.
    pub fn retain_imports(&mut self, retained: &Self) -> Result<(), String> {
        for prior in retained.rounds.iter().filter(|r| r.imported) {
            if let Some(at) = self.rounds.iter().position(|r| r.id == prior.id) {
                if self.rounds[at].source != prior.source
                    || self.rounds[at].contract != prior.contract
                    || self.rounds[at].correction != prior.correction
                {
                    return Err("review.ledger: conflicting retained import".into());
                }
                self.rounds[at] = prior.clone();
            } else {
                self.rounds.push(prior.clone());
            }
        }
        self.validate()
    }
    /// Retain an unreadable supplied import without dropping any known usage.
    /// The host durably binds its original archive identity before admission.
    pub fn import_unknown(
        &mut self,
        id: &str,
        source: &str,
        contract: &str,
        correction: bool,
    ) -> Result<(), String> {
        self.validate()?;
        if !identity(id) || !sha(source) || !sha(contract) {
            return Err("review.ledger: invalid imported identity".into());
        }
        if let Some(old) = self.rounds.iter_mut().find(|r| r.id == id) {
            if old.source != source || old.contract != contract || old.correction != correction {
                return Err("review.ledger: imported round context conflict".into());
            }
            if !old.imported && old.usage.as_ref().is_none_or(|u| !u.closed) {
                return Err(
                    "review.ledger: import id belongs to an unfinished native owner".into(),
                );
            }
            old.uncertain = true;
            old.imported = true;
        } else {
            self.rounds.push(Round {
                id: id.into(),
                source: source.into(),
                contract: contract.into(),
                allowance: 0,
                correction,
                correction_claim: None,
                usage: None,
                uncertain: true,
                imported: true,
            });
        }
        self.validate()
    }
    /// Import observed parent accounting even when incomplete. Uncertainty is
    /// retained before returning an error, and only matching monotonic evidence
    /// can reconcile it. No reservation/admission is performed by this operation.
    pub fn import(
        &mut self,
        id: &str,
        source: &str,
        contract: &str,
        usage: ReviewUsage,
        correction: bool,
    ) -> Result<(), String> {
        usage.validate()?;
        self.validate()?;
        if !identity(id) || !sha(source) || !sha(contract) {
            return Err("review.ledger: invalid imported identity".into());
        }
        if let Some(old) = self.rounds.iter_mut().find(|r| r.id == id) {
            if old.source != source || old.contract != contract || old.correction != correction {
                return Err("review.ledger: imported round context conflict".into());
            }
            if old.usage.as_ref().is_some_and(|u| {
                !u.started.is_subset(&usage.started)
                    || !u.finished.is_subset(&usage.finished)
                    || u.closed && u != &usage
            }) || !old.imported && old.usage.as_ref() != Some(&usage)
            {
                old.uncertain = true;
                return Err("review.ledger: imported history contradicts retained usage".into());
            }
            old.allowance = old.allowance.max(usage.calls());
            old.usage = Some(usage.clone());
            old.uncertain = !usage.closed;
        } else {
            self.rounds.push(Round {
                id: id.into(),
                source: source.into(),
                contract: contract.into(),
                allowance: usage.calls(),
                correction,
                correction_claim: None,
                usage: Some(usage.clone()),
                uncertain: !usage.closed,
                imported: true,
            });
        }
        self.validate()?;
        if !usage.closed {
            return Err("review.ledger: imported usage is uncertain; retain prior allowance and reconcile before admission".into());
        }
        Ok(())
    }
}
