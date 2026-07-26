//! On-demand entity detail for inspectors (Phase 2, per ARCHITECTURE.md).
//!
//! The 10 Hz snapshot stays lean; inspectors fetch one entity by id when
//! the player opens it. [`AgentDetail`] carries everything the agent
//! inspector shows: identity, traits, the private stores (memories,
//! relations, beliefs — rendered with names), and the agent's recent
//! decision records with their explanations verbatim — the "why did you
//! do that?" the Phase 2 acceptance demands.

use crate::ids::{AgentId, ContractId};
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

/// Everything the contract view shows for one contract: terms, tallies,
/// the complete negotiation log (every offer, counteroffer and reason —
/// BRIEF §Contracts), and the delivery/miss/breach history.
#[derive(Clone, Debug, Serialize)]
pub struct ContractDetail {
    pub id: u32,
    pub seller: String,
    pub buyer: String,
    pub good: String,
    pub qty: i64,
    pub unit_price_cents: i64,
    pub state: String,
    pub start_tick: u64,
    pub next_due: u64,
    pub deliveries: u32,
    pub delivered: u32,
    pub missed: u32,
    pub delivered_units: i64,
    pub paid_total_cents: i64,
    pub penalties_cents: i64,
    /// The table, move by move. Empty for contracts whose negotiation has
    /// scrolled out of the journal ring.
    pub negotiation: Vec<NegotiationRow>,
    /// Delivery / miss / breach / termination / completion events, oldest
    /// first (bounded by the events ring).
    pub history: Vec<TickText>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NegotiationRow {
    pub by: &'static str,
    pub price_cents: i64,
    pub because: &'static str,
}

impl ContractDetail {
    pub fn capture(world: &World, id: ContractId) -> Option<ContractDetail> {
        let state = &world.state;
        let c = state.contracts.get(&id)?;
        let negotiation = world
            .journal
            .negotiations
            .iter()
            .find(|n| {
                matches!(
                    n.outcome,
                    crate::negotiation::NegotiationOutcome::Signed { contract } if contract == id
                )
            })
            .map(|n| {
                n.rounds
                    .iter()
                    .map(|r| NegotiationRow {
                        by: r.by.label(),
                        price_cents: r.unit_price.cents(),
                        because: r.because.label(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let history = world
            .journal
            .events
            .iter()
            .filter(|e| contract_of(&e.event) == Some(id))
            .map(|e| TickText {
                tick: e.tick,
                text: crate::snapshot::event_text(state, &e.event),
            })
            .collect();
        Some(ContractDetail {
            id: id.0,
            seller: business_label(state, c.seller),
            buyer: business_label(state, c.buyer),
            good: c.good.name().to_string(),
            qty: c.qty,
            unit_price_cents: c.unit_price.cents(),
            state: crate::snapshot::contract_state_label(c.state).to_string(),
            start_tick: c.start_tick,
            next_due: c.next_due,
            deliveries: c.deliveries,
            delivered: c.delivered,
            missed: c.missed,
            delivered_units: c.delivered_units,
            paid_total_cents: c.paid_total.cents(),
            penalties_cents: c.penalties_paid_total.cents(),
            negotiation,
            history,
        })
    }
}

/// The contract an event belongs to, if any.
fn contract_of(event: &crate::events::Event) -> Option<ContractId> {
    use crate::events::Event;
    match event {
        Event::ContractSigned { contract, .. }
        | Event::ContractDelivered { contract, .. }
        | Event::ContractMissed { contract, .. }
        | Event::ContractBreached { contract, .. }
        | Event::ContractTerminated { contract, .. }
        | Event::ContractCompleted { contract } => Some(*contract),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldConfig;

    #[test]
    fn contract_detail_carries_the_negotiation_and_history() {
        let mut w = World::from_config(WorldConfig::default_with_seed(42));
        w.run_ticks(200).unwrap();
        // Seed 42 signs organic contracts well before day 200.
        let cid = *w
            .state
            .contracts
            .keys()
            .next()
            .expect("organic contracts exist by day 200");
        let d = ContractDetail::capture(&w, cid).unwrap();
        assert!(!d.seller.is_empty() && !d.buyer.is_empty());
        assert!(
            d.negotiation.len() >= 2,
            "the signed contract's table is on the record"
        );
        assert!(
            d.negotiation.iter().any(|r| r.because.contains("opened")),
            "the opening bid is logged"
        );
        assert!(
            !d.history.is_empty(),
            "signing/delivery events appear in the history"
        );
        assert!(ContractDetail::capture(&w, ContractId(9999)).is_none());
    }

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
