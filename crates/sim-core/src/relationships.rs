//! Private dyadic relationships (Phase 2, per AGENT_DESIGN.md).
//!
//! Seven integer dimensions per known counterparty, updated in small
//! bounded steps at interaction sites (payroll, hiring, firing, wage
//! changes, takeovers, tenure) and drifting slowly back toward neutral in
//! the memory/relationships phase. Relations are private — an agent may
//! trust someone the town despises; public reputation is a separate
//! system. Sparse and bounded: only counterparties actually interacted
//! with are stored, and the most-neutral relation is evicted when the map
//! is full.
//!
//! The v1 consumer: the job-switch premium. Trust, affection and
//! dependence toward the current employer's owner make a worker harder to
//! poach; resentment makes them easier (DECISIONS.md #024). Neutral
//! relations reproduce pre-relationship behavior exactly.

use crate::agent::Agent;
use crate::ids::AgentId;
use serde::{Deserialize, Serialize};

/// Most counterparties an agent keeps score on.
pub const RELATION_CAP: usize = 16;
/// Every dimension sits here for strangers and drifts back here with time.
pub const NEUTRAL: u8 = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    pub trust: u8,
    pub affection: u8,
    pub fear: u8,
    pub respect: u8,
    pub resentment: u8,
    pub dependence: u8,
    pub commercial_reliability: u8,
}

impl Relationship {
    pub const NEUTRAL_RELATION: Relationship = Relationship {
        trust: NEUTRAL,
        affection: NEUTRAL,
        fear: NEUTRAL,
        respect: NEUTRAL,
        resentment: NEUTRAL,
        dependence: NEUTRAL,
        commercial_reliability: NEUTRAL,
    };

    /// How far from neutral this relation is, across all dimensions —
    /// the eviction key (smallest goes first) and the "is this worth
    /// remembering" measure.
    pub fn intensity(&self) -> i64 {
        [
            self.trust,
            self.affection,
            self.fear,
            self.respect,
            self.resentment,
            self.dependence,
            self.commercial_reliability,
        ]
        .iter()
        .map(|&d| (i64::from(d) - i64::from(NEUTRAL)).abs())
        .sum()
    }

    /// The attachment a worker feels toward an employer's owner, as a
    /// premium adjustment in basis points: trust + affection + dependence
    /// bind, resentment repels. Neutral relations yield zero. Clamped to
    /// ±500 bp so personality and relations stay bounded influences.
    pub fn bond_premium_bp(&self) -> i64 {
        let raw = i64::from(self.trust) + i64::from(self.affection) + i64::from(self.dependence)
            - i64::from(self.resentment)
            - 100;
        (raw * 3).clamp(-500, 500)
    }
}

fn bump(value: u8, delta: i16) -> u8 {
    (i16::from(value) + delta).clamp(0, 100) as u8
}

/// Apply bounded deltas to the relation `agent` holds toward `other`,
/// creating it at neutral on first contact and evicting the most-neutral
/// relation if the map is full. Deltas are (dimension, step) pairs applied
/// with clamping to 0..=100.
pub fn relate(agent: &mut Agent, other: AgentId, update: impl FnOnce(&mut Relationship)) {
    if agent.relations.len() >= RELATION_CAP && !agent.relations.contains_key(&other) {
        if let Some((&weakest, _)) = agent
            .relations
            .iter()
            .min_by_key(|(id, r)| (r.intensity(), **id))
        {
            agent.relations.remove(&weakest);
        }
    }
    let r = agent
        .relations
        .entry(other)
        .or_insert(Relationship::NEUTRAL_RELATION);
    update(r);
}

/// The relation `agent` holds toward `other`; strangers are neutral.
pub fn relation_toward(agent: &Agent, other: AgentId) -> Relationship {
    agent
        .relations
        .get(&other)
        .copied()
        .unwrap_or(Relationship::NEUTRAL_RELATION)
}

/// Weekly drift (phase 10, on the agent's stagger day): every dimension
/// moves one step back toward neutral, and fully-neutral relations are
/// dropped — acquaintance fades in roughly a year without interaction.
pub fn drift(agent: &mut Agent) {
    for r in agent.relations.values_mut() {
        for d in [
            &mut r.trust,
            &mut r.affection,
            &mut r.fear,
            &mut r.respect,
            &mut r.resentment,
            &mut r.dependence,
            &mut r.commercial_reliability,
        ] {
            match (*d).cmp(&NEUTRAL) {
                std::cmp::Ordering::Greater => *d -= 1,
                std::cmp::Ordering::Less => *d += 1,
                std::cmp::Ordering::Equal => {}
            }
        }
    }
    agent.relations.retain(|_, r| r.intensity() > 0);
}

// --- Interaction-site updates (bounded steps, clamped) ---

/// A full day's wage arrived.
pub fn on_wage_paid(worker: &mut Agent, owner: AgentId) {
    relate(worker, owner, |r| {
        r.commercial_reliability = bump(r.commercial_reliability, 1);
    });
}

/// Payroll failed and the worker walked out.
pub fn on_unpaid(worker: &mut Agent, owner: AgentId) {
    relate(worker, owner, |r| {
        r.trust = bump(r.trust, -30);
        r.resentment = bump(r.resentment, 30);
        r.fear = bump(r.fear, 5);
        r.commercial_reliability = bump(r.commercial_reliability, -40);
    });
}

/// Hired (or re-hired).
pub fn on_hired(worker: &mut Agent, owner: AgentId) {
    relate(worker, owner, |r| {
        r.trust = bump(r.trust, 5);
        r.dependence = bump(r.dependence, 20);
    });
}

/// Let go in a cash crunch.
pub fn on_fired(worker: &mut Agent, owner: AgentId) {
    relate(worker, owner, |r| {
        r.trust = bump(r.trust, -10);
        r.resentment = bump(r.resentment, 20);
        r.fear = bump(r.fear, 15);
        r.dependence = bump(r.dependence, -30);
    });
}

/// Weekly tenure drip while employed: attachment grows slowly.
pub fn on_tenure_week(worker: &mut Agent, owner: AgentId) {
    relate(worker, owner, |r| {
        r.affection = bump(r.affection, 1);
        r.dependence = bump(r.dependence, 1);
    });
}

/// The employer moved the wage; workers notice which way.
pub fn on_wage_moved(worker: &mut Agent, owner: AgentId, raised: bool) {
    relate(worker, owner, |r| {
        if raised {
            r.respect = bump(r.respect, 2);
        } else {
            r.resentment = bump(r.resentment, 5);
        }
    });
}

/// A business changed hands; both parties met over the deal.
pub fn on_deal(party: &mut Agent, counterparty: AgentId) {
    relate(party, counterparty, |r| {
        r.respect = bump(r.respect, 10);
        r.trust = bump(r.trust, 5);
    });
}

/// Leaving a job voluntarily loosens dependence on the old owner.
pub fn on_left_job(worker: &mut Agent, old_owner: AgentId) {
    relate(worker, old_owner, |r| {
        r.dependence = bump(r.dependence, -30);
    });
}

/// A contract delivery settled cleanly: both owners find the other a
/// little more reliable to do business with (Phase 3 settlement site).
pub fn on_contract_delivered(party: &mut Agent, counterparty: AgentId) {
    relate(party, counterparty, |r| {
        r.commercial_reliability = bump(r.commercial_reliability, 2);
        r.trust = bump(r.trust, 1);
    });
}

/// A delivery was missed: the wronged owner's view of the failer sours —
/// sharper than the slow build of clean deliveries, softer than an unpaid
/// payroll (a business shortfall, not a personal betrayal).
pub fn on_contract_missed(victim: &mut Agent, failer: AgentId) {
    relate(victim, failer, |r| {
        r.commercial_reliability = bump(r.commercial_reliability, -8);
        r.trust = bump(r.trust, -4);
        r.resentment = bump(r.resentment, 4);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Traits;
    use crate::ids::AgentId;
    use crate::money::Money;

    fn agent() -> Agent {
        Agent {
            id: AgentId(0),
            name: "Test".into(),
            cash: Money::ZERO,
            pantry: 0,
            employer: None,
            owns: None,
            owns_home: false,
            traits: Traits::NEUTRAL,
            memories: Vec::new(),
            relations: std::collections::BTreeMap::new(),
            beliefs: std::collections::BTreeMap::new(),
            hungry_streak: 0,
            days_unemployed: 0,
            total_earned: Money::ZERO,
            total_spent: Money::ZERO,
        }
    }

    #[test]
    fn updates_clamp_and_strangers_are_neutral() {
        let mut a = agent();
        let boss = AgentId(9);
        assert_eq!(relation_toward(&a, boss), Relationship::NEUTRAL_RELATION);
        for _ in 0..10 {
            on_unpaid(&mut a, boss);
        }
        let r = relation_toward(&a, boss);
        assert_eq!(r.trust, 0, "clamped at the floor");
        assert_eq!(r.resentment, 100, "clamped at the ceiling");
        assert_eq!(
            relation_toward(&a, AgentId(8)),
            Relationship::NEUTRAL_RELATION
        );
    }

    #[test]
    fn drift_fades_relations_and_drops_the_fully_neutral() {
        let mut a = agent();
        let boss = AgentId(9);
        on_fired(&mut a, boss);
        let before = relation_toward(&a, boss).intensity();
        drift(&mut a);
        assert!(relation_toward(&a, boss).intensity() < before);
        for _ in 0..100 {
            drift(&mut a);
        }
        assert!(a.relations.is_empty(), "acquaintance fades entirely");
    }

    #[test]
    fn the_map_is_bounded_and_evicts_the_most_neutral() {
        let mut a = agent();
        for i in 0..RELATION_CAP as u32 {
            on_hired(&mut a, AgentId(i));
        }
        // One relation is nearly neutral: it should be evicted first.
        a.relations.get_mut(&AgentId(4)).unwrap().trust = NEUTRAL;
        a.relations.get_mut(&AgentId(4)).unwrap().dependence = NEUTRAL + 1;
        on_unpaid(&mut a, AgentId(99));
        assert_eq!(a.relations.len(), RELATION_CAP);
        assert!(!a.relations.contains_key(&AgentId(4)));
        assert!(a.relations.contains_key(&AgentId(99)));
    }

    #[test]
    fn bonds_bind_and_resentment_repels() {
        assert_eq!(Relationship::NEUTRAL_RELATION.bond_premium_bp(), 0);
        let mut loyal = Relationship::NEUTRAL_RELATION;
        loyal.trust = 80;
        loyal.affection = 70;
        loyal.dependence = 80;
        assert!(loyal.bond_premium_bp() > 0, "attachment raises the bar");
        let mut bitter = Relationship::NEUTRAL_RELATION;
        bitter.resentment = 90;
        bitter.trust = 20;
        assert!(bitter.bond_premium_bp() < 0, "resentment lowers it");
        let mut extreme = Relationship::NEUTRAL_RELATION;
        extreme.trust = 100;
        extreme.affection = 100;
        extreme.dependence = 100;
        extreme.resentment = 0;
        assert_eq!(extreme.bond_premium_bp(), 500, "clamped");
    }
}
