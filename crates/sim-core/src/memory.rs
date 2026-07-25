//! Agent memory (Phase 2, per AGENT_DESIGN.md §Memory).
//!
//! A bounded per-agent store. Memories form at the event sites that
//! produce them (never by reading the journal back), decay a little every
//! memory-phase tick, are reinforced by repetition (same kind, same
//! subject: confidence restored, importance strengthened — never a
//! duplicate entry), and are evicted weakest-first when the store is full.
//! Deterministic inaccuracy exists only as explicit confidence decay.
//!
//! Memories live in hashed state because decisions read them. The v1
//! consumer: grievances — a non-desperate agent will not work for a
//! business they remember being stiffed or fired by, until the memory
//! fades or desperation overrides pride (DECISIONS.md #023). The spec's
//! emotional/trust/financial impact fields arrive with relationships,
//! which consume them; adding them now would be decoration.

use crate::agent::Agent;
use crate::ids::BusinessId;
use serde::{Deserialize, Serialize};

/// Most memories an agent retains; the weakest (importance × confidence)
/// is evicted first, ties going to the oldest.
pub const MEMORY_CAP: usize = 12;
/// Confidence at formation and after reinforcement, in milli-units.
pub const CONFIDENCE_FULL: i64 = 1_000;
/// Confidence lost per memory-phase tick (full → forgotten in 500 days).
pub const CONFIDENCE_DECAY_PER_TICK: i64 = 2;
/// Reinforcement strengthens importance by this much, capped at 100.
pub const REINFORCE_IMPORTANCE: u8 = 10;
/// A grievance below this strength no longer changes behavior.
pub const GRIEVANCE_ACTIVE_STRENGTH: i64 = 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum MemoryKind {
    /// This business failed to pay my wages.
    UnpaidBy(BusinessId),
    /// This business let me go in a cash crunch.
    FiredBy(BusinessId),
}

impl MemoryKind {
    /// The business a grievance is held against.
    pub fn grievance_target(self) -> BusinessId {
        match self {
            MemoryKind::UnpaidBy(b) | MemoryKind::FiredBy(b) => b,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub kind: MemoryKind,
    /// When it happened (last reinforcement).
    pub tick: u64,
    /// How much it mattered at formation, 0–100.
    pub importance: u8,
    /// Fades every tick; the memory is forgotten at zero.
    pub confidence_milli: i64,
}

impl Memory {
    /// Effective weight: importance scaled by remaining confidence.
    pub fn strength(&self) -> i64 {
        i64::from(self.importance) * self.confidence_milli / CONFIDENCE_FULL
    }
}

/// Form (or reinforce) a memory. Repetition of the same kind restores
/// confidence and strengthens importance instead of duplicating; a new
/// memory evicts the weakest entry when the store is full.
pub fn remember(agent: &mut Agent, kind: MemoryKind, tick: u64, importance: u8) {
    if let Some(m) = agent.memories.iter_mut().find(|m| m.kind == kind) {
        m.tick = tick;
        m.confidence_milli = CONFIDENCE_FULL;
        m.importance = m.importance.saturating_add(REINFORCE_IMPORTANCE).min(100);
        return;
    }
    if agent.memories.len() >= MEMORY_CAP {
        // Evict the weakest; on ties the earliest entry (oldest) goes.
        if let Some((idx, _)) = agent
            .memories
            .iter()
            .enumerate()
            .min_by_key(|(i, m)| (m.strength(), *i))
        {
            agent.memories.remove(idx);
        }
    }
    agent.memories.push(Memory {
        kind,
        tick,
        importance,
        confidence_milli: CONFIDENCE_FULL,
    });
}

/// Memory-phase decay: every memory fades a little; forgotten ones drop.
pub fn decay(agent: &mut Agent) {
    for m in agent.memories.iter_mut() {
        m.confidence_milli -= CONFIDENCE_DECAY_PER_TICK;
    }
    agent.memories.retain(|m| m.confidence_milli > 0);
}

/// Whether the agent still holds an active grievance against a business.
pub fn holds_grievance(agent: &Agent, business: BusinessId) -> bool {
    agent
        .memories
        .iter()
        .filter(|m| m.kind.grievance_target() == business)
        .map(Memory::strength)
        .sum::<i64>()
        >= GRIEVANCE_ACTIVE_STRENGTH
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
            hungry_streak: 0,
            days_unemployed: 0,
            total_earned: Money::ZERO,
            total_spent: Money::ZERO,
            memories: Vec::new(),
            relations: std::collections::BTreeMap::new(),
            beliefs: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn repetition_reinforces_instead_of_duplicating() {
        let mut a = agent();
        let kind = MemoryKind::UnpaidBy(BusinessId(3));
        remember(&mut a, kind, 10, 90);
        a.memories[0].confidence_milli = 300; // partially faded
        remember(&mut a, kind, 50, 90);
        assert_eq!(a.memories.len(), 1, "no duplicates");
        assert_eq!(a.memories[0].confidence_milli, CONFIDENCE_FULL);
        assert_eq!(a.memories[0].importance, 100, "90 + 10, capped");
        assert_eq!(a.memories[0].tick, 50);
    }

    #[test]
    fn decay_forgets_and_grievances_fade_below_threshold() {
        let mut a = agent();
        remember(&mut a, MemoryKind::FiredBy(BusinessId(1)), 0, 70);
        assert!(holds_grievance(&a, BusinessId(1)));
        assert!(!holds_grievance(&a, BusinessId(2)));
        // strength = 70 × conf/1000; falls under 20 when conf < ~286.
        for _ in 0..400 {
            decay(&mut a);
        }
        assert!(!a.memories.is_empty(), "still faintly remembered");
        assert!(!holds_grievance(&a, BusinessId(1)), "too faded to act on");
        for _ in 0..200 {
            decay(&mut a);
        }
        assert!(a.memories.is_empty(), "fully forgotten");
    }

    #[test]
    fn eviction_removes_the_weakest_first() {
        let mut a = agent();
        for i in 0..MEMORY_CAP {
            remember(
                &mut a,
                MemoryKind::FiredBy(BusinessId(i as u32)),
                i as u64,
                50,
            );
        }
        // One faded near oblivion: it is the weakest.
        a.memories[4].confidence_milli = 10;
        remember(&mut a, MemoryKind::UnpaidBy(BusinessId(99)), 100, 80);
        assert_eq!(a.memories.len(), MEMORY_CAP);
        assert!(
            !a.memories
                .iter()
                .any(|m| m.kind == MemoryKind::FiredBy(BusinessId(4))),
            "the weakest memory was evicted"
        );
        assert!(a
            .memories
            .iter()
            .any(|m| m.kind == MemoryKind::UnpaidBy(BusinessId(99))));
    }
}
