//! Public reputation as propagated belief (Phase 2, per AGENT_DESIGN.md).
//!
//! Reputation is not a global score: each agent holds bounded BELIEFS
//! about others, formed by direct observation at event sites and spread
//! through a deterministic rumor channel — weekly workplace gossip, where
//! a listener's beliefs move a quarter-step toward each colleague's. Bad
//! news travels with people: a stiffed worker rehired elsewhere seeds a
//! whole new roster's opinion of the old owner. An agent may privately
//! trust someone the town despises — beliefs and relationships are
//! separate stores.
//!
//! Dimensions with live drivers today: **reliable** (payroll observed /
//! payroll missed), **generous** (wage raises and cuts), **ruthless**
//! (firings). The spec's remaining dimensions (honest, competent, wealthy,
//! dangerous, influential, corrupt) arrive with their drivers — contract
//! performance (Phase 3) and politics (Phase 4); adding them now would be
//! decoration (DECISIONS.md #025).
//!
//! The v1 consumer: a non-desperate job seeker refuses an owner they
//! believe unreliable. `probe_reputation` guards the propagation channel.

use crate::agent::Agent;
use crate::ids::AgentId;
use serde::{Deserialize, Serialize};

/// Most subjects an agent keeps beliefs about.
pub const BELIEF_CAP: usize = 16;
/// Neutral opinion; strangers and the unheard-of sit here.
pub const NEUTRAL: u8 = 50;
/// A seeker refuses employers believed less reliable than this.
pub const RELIABLE_HIRING_FLOOR: u8 = 26;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Belief {
    pub reliable: u8,
    pub generous: u8,
    pub ruthless: u8,
}

impl Belief {
    pub const NEUTRAL_BELIEF: Belief = Belief {
        reliable: NEUTRAL,
        generous: NEUTRAL,
        ruthless: NEUTRAL,
    };

    /// Distance from neutral — the eviction key (smallest first).
    pub fn intensity(&self) -> i64 {
        [self.reliable, self.generous, self.ruthless]
            .iter()
            .map(|&d| (i64::from(d) - i64::from(NEUTRAL)).abs())
            .sum()
    }
}

fn bump(value: u8, delta: i16) -> u8 {
    (i16::from(value) + delta).clamp(0, 100) as u8
}

/// Update the belief `agent` holds about `subject`, creating it at neutral
/// on first news and evicting the most-neutral belief when full.
pub fn believe(agent: &mut Agent, subject: AgentId, update: impl FnOnce(&mut Belief)) {
    if agent.beliefs.len() >= BELIEF_CAP && !agent.beliefs.contains_key(&subject) {
        if let Some((&weakest, _)) = agent
            .beliefs
            .iter()
            .min_by_key(|(id, b)| (b.intensity(), **id))
        {
            agent.beliefs.remove(&weakest);
        }
    }
    let b = agent
        .beliefs
        .entry(subject)
        .or_insert(Belief::NEUTRAL_BELIEF);
    update(b);
}

/// The belief `agent` holds about `subject`; the unheard-of are neutral.
pub fn belief_about(agent: &Agent, subject: AgentId) -> Belief {
    agent
        .beliefs
        .get(&subject)
        .copied()
        .unwrap_or(Belief::NEUTRAL_BELIEF)
}

/// Weekly drift toward neutral (phase 10, on the agent's stagger day);
/// fully-neutral beliefs drop — old news stops being news.
pub fn drift(agent: &mut Agent) {
    for b in agent.beliefs.values_mut() {
        for d in [&mut b.reliable, &mut b.generous, &mut b.ruthless] {
            match (*d).cmp(&NEUTRAL) {
                std::cmp::Ordering::Greater => *d -= 1,
                std::cmp::Ordering::Less => *d += 1,
                std::cmp::Ordering::Equal => {}
            }
        }
    }
    agent.beliefs.retain(|_, b| b.intensity() > 0);
}

/// The rumor channel: the listener's beliefs move a quarter of the gap
/// toward the speaker's, for every subject the speaker has an opinion on
/// (excluding either party). Listener-updates-only keeps the pass
/// deterministic under id-ordered iteration; the speaker converges on
/// their own gossip day. Integer steps round toward zero, so opinions
/// converge without oscillating.
pub fn gossip(listener: &mut Agent, listener_id: AgentId, speaker_beliefs: &[(AgentId, Belief)]) {
    for (subject, spoken) in speaker_beliefs {
        if *subject == listener_id {
            continue;
        }
        let spoken = *spoken;
        believe(listener, *subject, |mine| {
            mine.reliable = bump(
                mine.reliable,
                ((i16::from(spoken.reliable) - i16::from(mine.reliable)) / 4).clamp(-25, 25),
            );
            mine.generous = bump(
                mine.generous,
                ((i16::from(spoken.generous) - i16::from(mine.generous)) / 4).clamp(-25, 25),
            );
            mine.ruthless = bump(
                mine.ruthless,
                ((i16::from(spoken.ruthless) - i16::from(mine.ruthless)) / 4).clamp(-25, 25),
            );
        });
    }
}

// --- Direct-observation updates at event sites ---

/// Wages arrived today: slow public repair of an owner's reliability.
pub fn on_payroll_observed(worker: &mut Agent, owner: AgentId) {
    believe(worker, owner, |b| {
        b.reliable = bump(b.reliable, 1);
    });
}

/// Payroll failed: the victims know exactly who did this.
pub fn on_missed_payroll_victim(worker: &mut Agent, owner: AgentId) {
    believe(worker, owner, |b| {
        b.reliable = bump(b.reliable, -25);
    });
}

/// Let go in a cash crunch: the victim learns something public too.
pub fn on_fired_victim(worker: &mut Agent, owner: AgentId) {
    believe(worker, owner, |b| {
        b.ruthless = bump(b.ruthless, 20);
    });
}

/// The wage moved; the roster judges the owner's generosity.
pub fn on_wage_moved_witness(worker: &mut Agent, owner: AgentId, raised: bool) {
    believe(worker, owner, |b| {
        b.generous = bump(b.generous, if raised { 5 } else { -5 });
    });
}

/// A contract delivery was missed: the wronged owner now believes the
/// failing owner unreliable — contract performance is a reputation driver
/// (BRIEF.md), and gossip carries the news from here. Softer than a missed
/// payroll (−25): one failed shipment, not a roster left unpaid.
pub fn on_contract_missed_victim(victim: &mut Agent, failer: AgentId) {
    believe(victim, failer, |b| {
        b.reliable = bump(b.reliable, -15);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Traits;
    use crate::money::Money;

    fn agent(id: u32) -> Agent {
        Agent {
            id: AgentId(id),
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
    fn observation_forms_beliefs_and_drift_forgets_them() {
        let mut a = agent(0);
        let owner = AgentId(9);
        on_missed_payroll_victim(&mut a, owner);
        assert_eq!(belief_about(&a, owner).reliable, 25);
        for _ in 0..30 {
            drift(&mut a);
        }
        assert!(a.beliefs.is_empty(), "old news stops being news");
    }

    #[test]
    fn gossip_moves_the_listener_a_quarter_of_the_gap() {
        let mut listener = agent(0);
        let owner = AgentId(9);
        // The speaker knows the owner is unreliable (25); listener neutral.
        let speaker_beliefs = vec![(
            owner,
            Belief {
                reliable: 25,
                generous: NEUTRAL,
                ruthless: NEUTRAL,
            },
        )];
        gossip(&mut listener, AgentId(0), &speaker_beliefs);
        // (25 − 50) / 4 = −6 (toward zero): 50 → 44.
        assert_eq!(belief_about(&listener, owner).reliable, 44);
        gossip(&mut listener, AgentId(0), &speaker_beliefs);
        assert_eq!(belief_about(&listener, owner).reliable, 40);
        // Nobody gossips the listener about themselves.
        let about_me = vec![(AgentId(0), Belief::NEUTRAL_BELIEF)];
        gossip(&mut listener, AgentId(0), &about_me);
        assert!(!listener.beliefs.contains_key(&AgentId(0)));
    }

    #[test]
    fn the_belief_store_is_bounded_with_most_neutral_eviction() {
        let mut a = agent(0);
        for i in 0..BELIEF_CAP as u32 {
            believe(&mut a, AgentId(i), |b| {
                b.ruthless = bump(b.ruthless, 20);
            });
        }
        a.beliefs.get_mut(&AgentId(4)).unwrap().ruthless = NEUTRAL + 1;
        believe(&mut a, AgentId(99), |b| {
            b.reliable = bump(b.reliable, -25);
        });
        assert_eq!(a.beliefs.len(), BELIEF_CAP);
        assert!(!a.beliefs.contains_key(&AgentId(4)));
        assert!(a.beliefs.contains_key(&AgentId(99)));
    }
}
