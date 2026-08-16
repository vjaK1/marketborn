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

/// Everything the business inspector shows for one business: identity and
/// staffing, pricing and stock, the full lifetime books (the cash-basis
/// P&L the `business_books` invariant reconciles every sweep), a balance
/// sheet at market valuation, credit standing, its contracts on both
/// sides, and its recent event history (bounded by the events ring).
#[derive(Clone, Debug, Serialize)]
pub struct BusinessDetail {
    pub id: u32,
    pub name: String,
    pub kind: String,
    pub owner: String,
    pub owner_id: u32,
    pub sells: String,
    pub price_cents: i64,
    pub wage_cents: i64,
    pub workers: Vec<String>,
    pub target_headcount: u32,
    pub expected_daily_sales: i64,
    pub stockout_days: u32,
    pub last_window_profit_cents: i64,
    pub lifetime_profit_cents: i64,
    pub inventory: Vec<InventoryRow>,
    // --- Balance sheet (inventory at last market prices — a derived
    // view, the same valuation takeovers pay). ---
    pub cash_cents: i64,
    pub inventory_value_cents: i64,
    pub assets_cents: i64,
    pub liabilities_cents: i64,
    pub equity_cents: i64,
    // --- Lifetime cash-basis books, categorized at their ledger sites. ---
    pub books: Vec<NamedMoney>,
    pub spoiled_units: i64,
    pub seized_units: i64,
    pub loan: Option<LoanRow>,
    pub prior_defaults: u32,
    pub contracts: Vec<BizContractRow>,
    /// Newest `BUSINESS_EVENT_TAIL` events touching this business, oldest
    /// first (honestly bounded by the events ring).
    pub history: Vec<TickText>,
}

#[derive(Clone, Debug, Serialize)]
pub struct InventoryRow {
    pub good: String,
    pub qty: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct NamedMoney {
    pub name: &'static str,
    pub cents: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LoanRow {
    pub id: u32,
    pub principal_cents: i64,
    pub outstanding_cents: i64,
    pub rate_bp: i64,
    pub missed_payments: u32,
    pub start_tick: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BizContractRow {
    pub id: u32,
    pub role: &'static str,
    pub counterparty: String,
    pub good: String,
    pub qty: i64,
    pub unit_price_cents: i64,
    pub state: String,
    pub delivered: u32,
    pub deliveries: u32,
}

/// Most recent events returned per business (price reviews and dividends
/// accumulate fast; the ring holds far more than a reader wants).
const BUSINESS_EVENT_TAIL: usize = 40;

impl BusinessDetail {
    pub fn capture(world: &World, id: crate::ids::BusinessId) -> Option<BusinessDetail> {
        let state = &world.state;
        let b = state.businesses.get(&id)?;
        let inventory = b
            .inventory
            .iter()
            .filter(|(_, qty)| **qty != 0)
            .map(|(good, qty)| InventoryRow {
                good: good.name().to_string(),
                qty: *qty,
            })
            .collect();
        let inventory_value = b.inventory_value(&state.market.last_prices);
        let loan = state.bank.active_loan_of(id).map(|l| LoanRow {
            id: l.id.0,
            principal_cents: l.principal.cents(),
            outstanding_cents: l.outstanding.cents(),
            rate_bp: l.rate_bp,
            missed_payments: l.missed_payments,
            start_tick: l.start_tick,
        });
        let liabilities = state
            .bank
            .active_loan_of(id)
            .map(|l| l.outstanding)
            .unwrap_or(crate::money::Money::ZERO);
        let prior_defaults = state
            .bank
            .loans
            .values()
            .filter(|l| l.state == crate::bank::LoanState::Defaulted && l.borrower == id)
            .count() as u32;
        let bk = &b.books;
        let books = vec![
            NamedMoney {
                name: "starting cash",
                cents: bk.starting_cash.cents(),
            },
            NamedMoney {
                name: "revenue",
                cents: bk.revenue.cents(),
            },
            NamedMoney {
                name: "penalties received",
                cents: bk.penalties_received.cents(),
            },
            NamedMoney {
                name: "owner investment",
                cents: bk.owner_invested.cents(),
            },
            NamedMoney {
                name: "loans received",
                cents: bk.loan_received.cents(),
            },
            NamedMoney {
                name: "monetary policy (net)",
                cents: bk.policy_net.cents(),
            },
            NamedMoney {
                name: "input costs",
                cents: -bk.input_costs.cents(),
            },
            NamedMoney {
                name: "tool costs",
                cents: -bk.tool_costs.cents(),
            },
            NamedMoney {
                name: "wages",
                cents: -bk.wages.cents(),
            },
            NamedMoney {
                name: "sales tax remitted",
                cents: -bk.taxes_paid.cents(),
            },
            NamedMoney {
                name: "loan interest",
                cents: -bk.interest_paid.cents(),
            },
            NamedMoney {
                name: "principal repaid",
                cents: -bk.principal_repaid.cents(),
            },
            NamedMoney {
                name: "penalties paid",
                cents: -bk.penalties_paid.cents(),
            },
            NamedMoney {
                name: "dividends",
                cents: -bk.dividends.cents(),
            },
            NamedMoney {
                name: "seized in foreclosure",
                cents: -bk.seized_cash.cents(),
            },
        ];
        let contracts = state
            .contracts
            .values()
            .filter(|c| c.seller == id || c.buyer == id)
            .map(|c| {
                let (role, other) = if c.seller == id {
                    ("supplier", c.buyer)
                } else {
                    ("buyer", c.seller)
                };
                BizContractRow {
                    id: c.id.0,
                    role,
                    counterparty: business_label(state, other),
                    good: c.good.name().to_string(),
                    qty: c.qty,
                    unit_price_cents: c.unit_price.cents(),
                    state: crate::snapshot::contract_state_label(c.state).to_string(),
                    delivered: c.delivered,
                    deliveries: c.deliveries,
                }
            })
            .collect();
        let mut history: Vec<TickText> = world
            .journal
            .events
            .iter()
            .rev()
            .filter(|e| event_touches_business(state, &e.event, id))
            .take(BUSINESS_EVENT_TAIL)
            .map(|e| TickText {
                tick: e.tick,
                text: crate::snapshot::event_text(state, &e.event),
            })
            .collect();
        history.reverse();
        Some(BusinessDetail {
            id: id.0,
            name: b.name.clone(),
            kind: b.kind.label().to_string(),
            owner: agent_label(state, b.owner),
            owner_id: b.owner.0,
            sells: b.sells.name().to_string(),
            price_cents: b.price.cents(),
            wage_cents: b.wage.cents(),
            workers: b.workers.iter().map(|w| agent_label(state, *w)).collect(),
            target_headcount: b.target_headcount,
            expected_daily_sales: b.expected_daily_sales(),
            stockout_days: b.stockout_days,
            last_window_profit_cents: b.last_window_profit.cents(),
            lifetime_profit_cents: bk.lifetime_profit().cents(),
            inventory,
            cash_cents: b.cash.cents(),
            inventory_value_cents: inventory_value.cents(),
            assets_cents: (b.cash + inventory_value).cents(),
            liabilities_cents: liabilities.cents(),
            equity_cents: (b.cash + inventory_value - liabilities).cents(),
            books,
            spoiled_units: bk.spoiled_units,
            seized_units: bk.seized_units,
            loan,
            prior_defaults,
            contracts,
            history,
        })
    }
}

/// Whether an event belongs to this business's story — direct fields, and
/// contract events resolved through the contract's parties.
fn event_touches_business(
    state: &crate::world::SimState,
    event: &crate::events::Event,
    id: crate::ids::BusinessId,
) -> bool {
    use crate::events::Event;
    match event {
        Event::Hired { business, .. }
        | Event::Fired { business, .. }
        | Event::QuitUnpaid { business, .. }
        | Event::MissedPayroll { business, .. }
        | Event::PriceChanged { business, .. }
        | Event::WageChanged { business, .. }
        | Event::DividendPaid { business, .. }
        | Event::OwnerInvested { business, .. }
        | Event::BusinessSold { business, .. }
        | Event::LoanIssued { business, .. }
        | Event::LoanPaymentMissed { business, .. }
        | Event::LoanRepaid { business, .. }
        | Event::LoanDefaulted { business, .. }
        | Event::CollateralSeized { business, .. } => *business == id,
        Event::JobSwitched { from, to, .. } => *from == id || *to == id,
        Event::ContractSigned { contract, .. }
        | Event::ContractDelivered { contract, .. }
        | Event::ContractMissed { contract, .. }
        | Event::ContractBreached { contract, .. }
        | Event::ContractTerminated { contract, .. }
        | Event::ContractCompleted { contract } => state
            .contracts
            .get(contract)
            .is_some_and(|c| c.seller == id || c.buyer == id),
        _ => false,
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
    fn business_detail_carries_books_balance_sheet_and_history() {
        let mut w = World::from_config(WorldConfig::default_with_seed(42));
        w.run_ticks(200).unwrap();
        let bid = *w.state.businesses.keys().next().unwrap();
        let d = BusinessDetail::capture(&w, bid).unwrap();
        assert!(!d.name.is_empty() && !d.owner.is_empty());
        assert!(!d.workers.is_empty(), "the farm is staffed at day 200");
        let revenue = d.books.iter().find(|r| r.name == "revenue").unwrap();
        assert!(revenue.cents > 0, "200 days of sales are on the books");
        assert_eq!(
            d.equity_cents,
            d.assets_cents - d.liabilities_cents,
            "the balance sheet balances"
        );
        assert!(!d.history.is_empty(), "price reviews leave a trail");
        assert!(
            d.history.len() <= super::BUSINESS_EVENT_TAIL,
            "the tail is capped"
        );
        assert!(BusinessDetail::capture(&w, crate::ids::BusinessId(9999)).is_none());
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
