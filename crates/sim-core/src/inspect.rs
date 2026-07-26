//! On-demand entity detail for inspectors (Phase 2, per ARCHITECTURE.md).
//!
//! The 10 Hz snapshot stays lean; inspectors fetch one entity by id when
//! the player opens it. [`AgentDetail`] carries everything the agent
//! inspector shows: identity, traits, the private stores (memories,
//! relations, beliefs — rendered with names), and the agent's recent
//! decision records with their explanations verbatim — the "why did you
//! do that?" the Phase 2 acceptance demands.

use crate::ids::AgentId;
use crate::memory::MemoryKind;
use crate::snapshot::{agent_label, business_label};
use crate::world::World;
use serde::Serialize;

/// Most recent decision records returned per agent.
const DECISION_TAIL: usize = 10;

#[derive(Clone, Debug, Serialize)]
pub struct AgentDetail {
    pub id: u32,
    pub name: String,
    pub role: String,
    pub workplace: Option<String>,
    pub cash_cents: i64,
    pub pantry: i64,
    pub owns_home: bool,
    pub hungry_streak: u32,
    pub days_unemployed: u32,
    pub total_earned_cents: i64,
    pub total_spent_cents: i64,
    pub traits: Vec<NamedValue>,
    pub memories: Vec<TickText>,
    pub relations: Vec<RelationRow>,
    pub beliefs: Vec<BeliefRow>,
    /// Newest first; `text` is `DecisionRecord::explanation()` verbatim.
    pub decisions: Vec<TickText>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NamedValue {
    pub name: &'static str,
    pub value: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct TickText {
    pub tick: u64,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RelationRow {
    pub toward: String,
    pub trust: u8,
    pub affection: u8,
    pub fear: u8,
    pub respect: u8,
    pub resentment: u8,
    pub dependence: u8,
    pub commercial_reliability: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct BeliefRow {
    pub about: String,
    pub reliable: u8,
    pub generous: u8,
    pub ruthless: u8,
}

impl AgentDetail {
    pub fn capture(world: &World, id: AgentId) -> Option<AgentDetail> {
        let state = &world.state;
        let a = state.agents.get(&id)?;
        let t = a.traits;
        let traits = vec![
            NamedValue {
                name: "risk tolerance",
                value: t.risk_tolerance,
            },
            NamedValue {
                name: "time preference",
                value: t.time_preference,
            },
            NamedValue {
                name: "loyalty",
                value: t.loyalty,
            },
            NamedValue {
                name: "honesty",
                value: t.honesty,
            },
            NamedValue {
                name: "ambition",
                value: t.ambition,
            },
            NamedValue {
                name: "aggression",
                value: t.aggression,
            },
            NamedValue {
                name: "patience",
                value: t.patience,
            },
            NamedValue {
                name: "empathy",
                value: t.empathy,
            },
            NamedValue {
                name: "greed",
                value: t.greed,
            },
        ];
        let memories = a
            .memories
            .iter()
            .map(|m| {
                let what = match m.kind {
                    MemoryKind::UnpaidBy(b) => {
                        format!("Was left unpaid by {}", business_label(state, b))
                    }
                    MemoryKind::FiredBy(b) => {
                        format!("Was let go by {}", business_label(state, b))
                    }
                };
                TickText {
                    tick: m.tick,
                    text: format!(
                        "{what} (importance {}, confidence {}%)",
                        m.importance,
                        m.confidence_milli / 10
                    ),
                }
            })
            .collect();
        let relations = a
            .relations
            .iter()
            .map(|(other, r)| RelationRow {
                toward: agent_label(state, *other),
                trust: r.trust,
                affection: r.affection,
                fear: r.fear,
                respect: r.respect,
                resentment: r.resentment,
                dependence: r.dependence,
                commercial_reliability: r.commercial_reliability,
            })
            .collect();
        let beliefs = a
            .beliefs
            .iter()
            .map(|(subject, b)| BeliefRow {
                about: agent_label(state, *subject),
                reliable: b.reliable,
                generous: b.generous,
                ruthless: b.ruthless,
            })
            .collect();
        let decisions = world
            .journal
            .decisions
            .iter()
            .rev()
            .filter(|d| d.actor == id)
            .take(DECISION_TAIL)
            .map(|d| TickText {
                tick: d.tick,
                text: d.explanation(),
            })
            .collect();
        Some(AgentDetail {
            id: id.0,
            name: a.name.clone(),
            role: a.role_label().to_string(),
            workplace: a.owns.or(a.employer).map(|bid| business_label(state, bid)),
            cash_cents: a.cash.cents(),
            pantry: a.pantry,
            owns_home: a.owns_home,
            hungry_streak: a.hungry_streak,
            days_unemployed: a.days_unemployed,
            total_earned_cents: a.total_earned.cents(),
            total_spent_cents: a.total_spent.cents(),
            traits,
            memories,
            relations,
            beliefs,
            decisions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldConfig;

    #[test]
    fn detail_carries_a_real_decision_explanation() {
        let mut w = World::from_config(WorldConfig::default_with_seed(6));
        w.run_ticks(30).unwrap();
        // Every business owner has had price reviews by day 30.
        let owner = w.state.businesses.values().next().map(|b| b.owner).unwrap();
        let d = AgentDetail::capture(&w, owner).unwrap();
        assert_eq!(d.traits.len(), 9);
        assert!(
            !d.decisions.is_empty(),
            "owners accumulate decision records"
        );
        assert!(
            d.decisions.iter().any(|t| t.text.contains("price")),
            "a price-review explanation is present verbatim"
        );
        // Workers accumulate relations toward their owner by day 30.
        let worker = w
            .state
            .agents
            .values()
            .find(|a| a.employer.is_some())
            .map(|a| a.id)
            .unwrap();
        let wd = AgentDetail::capture(&w, worker).unwrap();
        assert!(!wd.relations.is_empty(), "payroll builds relations");
        assert!(AgentDetail::capture(&w, AgentId(9999)).is_none());
    }
}
