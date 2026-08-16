//! Outbound UI protocol: throttled world summaries.
//!
//! A [`WorldSnapshot`] is a compact, render-ready view — never the full
//! world. The shell emits these at ≤ 10 Hz; inspectors will use on-demand
//! detail queries from project Phase 2 onward.

use crate::events::Event;
use crate::goods::Good;
use crate::ids::{AccountId, AgentId, BusinessId};
use crate::world::{SimState, SimStatus, World};
use serde::Serialize;

const EVENT_TAIL: usize = 120;
/// Newest contracts carried in the snapshot table (full archive via the
/// detail protocol).
const CONTRACT_TAIL: usize = 50;
const HISTORY_DAYS: usize = 180;
/// Cosmetic calendar: 360-day years (DECISIONS.md #006).
const DAYS_PER_YEAR: u64 = 360;

#[derive(Clone, Debug, Serialize)]
pub struct WorldSnapshot {
    pub tick: u64,
    pub year: u64,
    pub day_of_year: u64,
    /// "running" or "halted: <reason>".
    pub status: String,
    pub stats: Stats,
    pub agents: Vec<AgentRow>,
    pub businesses: Vec<BusinessRow>,
    pub markets: Vec<MarketRow>,
    /// Newest first, capped — the contract view's table; the inspector
    /// fetches full detail (negotiation log, delivery history) by id.
    pub contracts: Vec<ContractRow>,
    pub price_history: PriceHistory,
    pub events: Vec<EventRow>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContractRow {
    pub id: u32,
    pub seller: String,
    pub buyer: String,
    pub good: String,
    /// Daily delivery ceiling (requirements form).
    pub qty: i64,
    pub unit_price_cents: i64,
    pub state: String,
    pub delivered: u32,
    pub missed: u32,
    pub deliveries: u32,
    pub start_tick: u64,
}

/// Per-good market view: standing depth (derived from the live offer/order
/// rules) plus the last completed day's outcomes from the metrics journal.
#[derive(Clone, Debug, Serialize)]
pub struct MarketRow {
    pub good: String,
    /// Volume-weighted average execution price of the most recent day with
    /// trades.
    pub last_price_cents: Option<i64>,
    pub volume_today: i64,
    pub unmet_today: i64,
    pub spoiled_today: i64,
    pub sellers: u32,
    pub offered_qty: i64,
    pub best_ask_cents: Option<i64>,
    pub demand_qty: i64,
    pub urgent_demand_qty: i64,
    /// Total units in the world (business inventories + pantries).
    pub world_stock: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Stats {
    pub population: u32,
    pub employed: u32,
    pub unemployed: u32,
    pub owners: u32,
    pub hungry: u32,
    pub money_total_cents: i64,
    pub food_price_cents: Option<i64>,
    /// Food currently on business shelves.
    pub food_stock: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentRow {
    pub id: u32,
    pub name: String,
    pub role: String,
    pub workplace: Option<String>,
    pub cash_cents: i64,
    pub pantry: i64,
    pub owns_home: bool,
    pub hungry_streak: u32,
    pub days_unemployed: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct BusinessRow {
    pub id: u32,
    pub name: String,
    pub kind: String,
    pub cash_cents: i64,
    pub workers: u32,
    pub target_workers: u32,
    pub wage_cents: i64,
    pub sells: String,
    pub price_cents: i64,
    pub output_stock: i64,
    pub input_stock: Vec<InputStockRow>,
    pub last_window_profit_cents: i64,
    pub sold_today: i64,
    pub produced_today: i64,
    pub books: BooksRow,
}

#[derive(Clone, Debug, Serialize)]
pub struct InputStockRow {
    pub good: String,
    pub qty: i64,
}

/// Derived accounting view: the lifetime cash-basis books plus a
/// balance-sheet valuation (inventory at last market prices, falling back
/// to the business's own posted price for its sold good).
#[derive(Clone, Debug, Serialize)]
pub struct BooksRow {
    pub revenue_cents: i64,
    pub input_costs_cents: i64,
    pub tool_costs_cents: i64,
    pub wages_cents: i64,
    pub dividends_cents: i64,
    pub owner_invested_cents: i64,
    pub lifetime_profit_cents: i64,
    pub spoiled_units: i64,
    pub inventory_value_cents: i64,
    /// Total assets: cash + inventory value (no liabilities until Phase 3).
    pub assets_cents: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PriceHistory {
    pub ticks: Vec<u64>,
    pub series: Vec<GoodSeries>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GoodSeries {
    pub good: String,
    pub points: Vec<Option<i64>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct EventRow {
    pub seq: u64,
    pub tick: u64,
    pub kind: String,
    pub text: String,
}

pub(crate) fn agent_label(state: &SimState, id: AgentId) -> String {
    state
        .agents
        .get(&id)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| id.to_string())
}

pub(crate) fn business_label(state: &SimState, id: BusinessId) -> String {
    state
        .businesses
        .get(&id)
        .map(|b| b.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn account_label(state: &SimState, account: AccountId) -> String {
    match account {
        AccountId::Agent(id) => agent_label(state, id),
        AccountId::Business(id) => business_label(state, id),
        AccountId::Bank => "the bank".to_string(),
        AccountId::Government => "the government".to_string(),
    }
}

/// `(seller, buyer)` labels for a contract, falling back to the bare id.
fn contract_parties(state: &SimState, id: crate::ids::ContractId) -> (String, String) {
    state
        .contracts
        .get(&id)
        .map(|c| {
            (
                business_label(state, c.seller),
                business_label(state, c.buyer),
            )
        })
        .unwrap_or_else(|| (id.to_string(), id.to_string()))
}

/// Human-readable rendering of an event against current state.
pub fn event_text(state: &SimState, event: &Event) -> String {
    match event {
        Event::WorldCreated {
            population,
            businesses,
        } => format!("The town is founded: {population} residents, {businesses} businesses"),
        Event::Hired {
            agent,
            business,
            wage,
        } => format!(
            "{} hired {} at {}/day",
            business_label(state, *business),
            agent_label(state, *agent),
            wage
        ),
        Event::Fired { agent, business } => format!(
            "{} let {} go in a cash crunch",
            business_label(state, *business),
            agent_label(state, *agent)
        ),
        Event::QuitUnpaid {
            agent,
            business,
            owed,
        } => format!(
            "{} walked out of {} over {} in unpaid wages",
            agent_label(state, *agent),
            business_label(state, *business),
            owed
        ),
        Event::MissedPayroll {
            business,
            workers_unpaid,
            shortfall,
        } => format!(
            "{} missed payroll for {workers_unpaid} worker(s), {shortfall} short",
            business_label(state, *business)
        ),
        Event::JobSwitched {
            agent,
            from,
            to,
            old_wage,
            new_wage,
        } => format!(
            "{} left {} for {} ({old_wage} → {new_wage}/day)",
            agent_label(state, *agent),
            business_label(state, *from),
            business_label(state, *to)
        ),
        Event::PriceChanged {
            business,
            good,
            old,
            new,
        } => {
            let verb = if new > old { "raised" } else { "cut" };
            format!(
                "{} {verb} its {good} price {old} → {new}",
                business_label(state, *business)
            )
        }
        Event::WageChanged { business, old, new } => {
            let verb = if new > old { "raised" } else { "lowered" };
            format!(
                "{} {verb} wages {old} → {new}",
                business_label(state, *business)
            )
        }
        Event::DividendPaid {
            business,
            owner,
            amount,
        } => format!(
            "{} paid a {amount} dividend to {}",
            business_label(state, *business),
            agent_label(state, *owner)
        ),
        Event::OwnerInvested {
            business,
            owner,
            amount,
        } => format!(
            "{} put {amount} of personal savings into {}",
            agent_label(state, *owner),
            business_label(state, *business)
        ),
        Event::BusinessSold {
            business,
            from,
            to,
            price,
        } => format!(
            "{} bought {} from {} for {price}",
            agent_label(state, *to),
            business_label(state, *business),
            agent_label(state, *from)
        ),
        Event::ContractSigned {
            contract,
            seller,
            buyer,
            good,
            qty,
            unit_price,
            deliveries,
        } => format!(
            "{} signed {contract}: {} supplies {qty} {good}/day at {unit_price} for {deliveries} days",
            business_label(state, *buyer),
            business_label(state, *seller)
        ),
        Event::ContractDelivered {
            contract,
            good,
            qty,
            amount,
        } => {
            let (seller, buyer) = contract_parties(state, *contract);
            format!("{seller} delivered {qty} {good} to {buyer} for {amount} under {contract}")
        }
        Event::ContractMissed {
            contract,
            by,
            penalty,
        } => {
            let (seller, buyer) = contract_parties(state, *contract);
            let failer = match by {
                crate::contracts::ContractParty::Seller => seller,
                crate::contracts::ContractParty::Buyer => buyer,
            };
            format!("{failer} missed a delivery under {contract}, paying a {penalty} penalty")
        }
        Event::ContractBreached { contract, by } => {
            let (seller, buyer) = contract_parties(state, *contract);
            let failer = match by {
                crate::contracts::ContractParty::Seller => seller,
                crate::contracts::ContractParty::Buyer => buyer,
            };
            format!("{contract} breached: {failer} failed three deliveries running")
        }
        Event::ContractTerminated {
            contract,
            by,
            penalty,
        } => {
            let (seller, buyer) = contract_parties(state, *contract);
            let (walker, other) = match by {
                crate::contracts::ContractParty::Seller => (seller, buyer),
                crate::contracts::ContractParty::Buyer => (buyer, seller),
            };
            format!("{walker} walked away from {contract} with {other}, paying a {penalty} exit penalty")
        }
        Event::ContractCompleted { contract } => {
            let (seller, buyer) = contract_parties(state, *contract);
            format!("{contract} between {seller} and {buyer} ran its full term")
        }
        Event::LoanIssued {
            loan,
            business,
            principal,
            rate_bp,
        } => format!(
            "The bank lent {} {principal} at {}% ({loan})",
            business_label(state, *business),
            rate_bp / 100
        ),
        Event::LoanPaymentMissed {
            loan,
            business,
            shortfall,
        } => format!(
            "{} missed a {loan} payment, {shortfall} short",
            business_label(state, *business)
        ),
        Event::LoanRepaid { loan, business } => format!(
            "{} repaid {loan} in full",
            business_label(state, *business)
        ),
        Event::LoanDefaulted {
            loan,
            business,
            outstanding,
        } => format!(
            "{} defaulted on {loan} owing {outstanding}",
            business_label(state, *business)
        ),
        Event::CollateralSeized {
            loan,
            business,
            cash,
            goods_value,
            written_off,
        } => format!(
            "The bank foreclosed on {} ({loan}): seized {cash} cash and {goods_value} in goods, wrote off {written_off}",
            business_label(state, *business)
        ),
        Event::BankRateSet { old_bp, new_bp } => {
            format!("The bank's base rate moved {}% → {}%", old_bp / 100, new_bp / 100)
        }
        Event::SalesTaxSet { old_bp, new_bp } => {
            format!("The sales tax moved {}% → {}%", old_bp / 100, new_bp / 100)
        }
        Event::ShockBegan { kind, days } => match kind {
            crate::shocks::ShockKind::Drought => format!(
                "A drought grips the farmland — the fields will yield half for {days} days"
            ),
        },
        Event::ShockEnded { kind } => match kind {
            crate::shocks::ShockKind::Drought => {
                "The drought has broken — the fields recover".to_string()
            }
        },
        Event::WelfarePaid { agent, amount } => format!(
            "The welfare office topped {} up with {amount}",
            agent_label(state, *agent)
        ),
        Event::AgentHungry { agent, streak } => {
            if *streak <= 1 {
                format!("{} went hungry today", agent_label(state, *agent))
            } else {
                format!(
                    "{} has been hungry for {streak} days",
                    agent_label(state, *agent)
                )
            }
        }
        Event::MonetaryPolicy {
            account,
            delta,
            memo,
        } => format!(
            "Monetary policy: {delta} at {} ({memo})",
            account_label(state, *account)
        ),
        Event::CommandRejected { seq, reason } => {
            format!("Command #{seq} rejected: {reason}")
        }
    }
}

impl WorldSnapshot {
    pub fn capture(world: &World) -> WorldSnapshot {
        let state = &world.state;
        let tick = state.tick;

        let mut employed = 0u32;
        let mut unemployed = 0u32;
        let mut owners = 0u32;
        let mut hungry = 0u32;
        for a in state.agents.values() {
            if a.owns.is_some() {
                owners += 1;
            } else if a.employer.is_some() {
                employed += 1;
            } else {
                unemployed += 1;
            }
            if a.hungry_streak > 0 {
                hungry += 1;
            }
        }

        let agents = state
            .agents
            .values()
            .map(|a| {
                let workplace = a.owns.or(a.employer).map(|bid| business_label(state, bid));
                AgentRow {
                    id: a.id.0,
                    name: a.name.clone(),
                    role: a.role_label().to_string(),
                    workplace,
                    cash_cents: a.cash.cents(),
                    pantry: a.pantry,
                    owns_home: a.owns_home,
                    hungry_streak: a.hungry_streak,
                    days_unemployed: a.days_unemployed,
                }
            })
            .collect();

        let businesses = state
            .businesses
            .values()
            .map(|b| {
                let mut input_stock: Vec<InputStockRow> = b
                    .recipe
                    .inputs
                    .iter()
                    .map(|(g, _)| InputStockRow {
                        good: g.name().to_string(),
                        qty: b.stock(*g),
                    })
                    .collect();
                // Tools aren't a recipe input, but tool users hold them on
                // site the same way — show them alongside inputs.
                if b.uses_tools {
                    input_stock.push(InputStockRow {
                        good: Good::Tools.name().to_string(),
                        qty: b.stock(Good::Tools),
                    });
                }
                let inventory_value_cents: i64 =
                    b.inventory_value(&state.market.last_prices).cents();
                let books = BooksRow {
                    revenue_cents: b.books.revenue.cents(),
                    input_costs_cents: b.books.input_costs.cents(),
                    tool_costs_cents: b.books.tool_costs.cents(),
                    wages_cents: b.books.wages.cents(),
                    dividends_cents: b.books.dividends.cents(),
                    owner_invested_cents: b.books.owner_invested.cents(),
                    lifetime_profit_cents: b.books.lifetime_profit().cents(),
                    spoiled_units: b.books.spoiled_units,
                    inventory_value_cents,
                    assets_cents: b.cash.cents() + inventory_value_cents,
                };
                BusinessRow {
                    id: b.id.0,
                    name: b.name.clone(),
                    kind: b.kind.label().to_string(),
                    cash_cents: b.cash.cents(),
                    workers: b.workers.len() as u32,
                    target_workers: b.target_headcount,
                    wage_cents: b.wage.cents(),
                    sells: b.sells.name().to_string(),
                    price_cents: b.price.cents(),
                    output_stock: b.stock(b.sells),
                    input_stock,
                    last_window_profit_cents: b.last_window_profit.cents(),
                    sold_today: b.sold_today,
                    produced_today: b.produced_today,
                    books,
                }
            })
            .collect();

        let last_day = world.journal.metrics.back();
        let markets = Good::ALL
            .iter()
            .map(|good| {
                // The standing market being described is the NEXT tick's,
                // whose contract reservations are the ones that will bind.
                let d = crate::market::depth(state, *good, state.tick + 1);
                let day_stat =
                    |f: &dyn Fn(&crate::metrics::MetricsDay) -> i64| last_day.map(f).unwrap_or(0);
                MarketRow {
                    good: good.name().to_string(),
                    last_price_cents: state.market.last_prices.get(good).map(|p| p.cents()),
                    volume_today: day_stat(&|m| m.volume.get(good).copied().unwrap_or(0)),
                    unmet_today: day_stat(&|m| m.unmet_demand.get(good).copied().unwrap_or(0)),
                    spoiled_today: day_stat(&|m| m.spoiled.get(good).copied().unwrap_or(0)),
                    sellers: d.sellers,
                    offered_qty: d.offered_qty,
                    best_ask_cents: d.best_ask.map(|p| p.cents()),
                    demand_qty: d.demand_qty,
                    urgent_demand_qty: d.urgent_demand_qty,
                    world_stock: state.total_goods(*good),
                }
            })
            .collect();

        let history_len = world.journal.metrics.len().min(HISTORY_DAYS);
        let skip = world.journal.metrics.len() - history_len;
        let window: Vec<_> = world.journal.metrics.iter().skip(skip).collect();
        let ticks = window.iter().map(|m| m.tick).collect();
        // The price chart shows flow goods; the Home asset trades too rarely
        // for a line (it stays in the markets table).
        let series = Good::ALL
            .iter()
            .filter(|good| **good != Good::Home)
            .map(|good| GoodSeries {
                good: good.name().to_string(),
                points: window
                    .iter()
                    .map(|m| m.avg_price.get(good).copied().flatten().map(|p| p.cents()))
                    .collect(),
            })
            .collect();

        let contracts = state
            .contracts
            .values()
            .rev()
            .take(CONTRACT_TAIL)
            .map(|c| ContractRow {
                id: c.id.0,
                seller: business_label(state, c.seller),
                buyer: business_label(state, c.buyer),
                good: c.good.name().to_string(),
                qty: c.qty,
                unit_price_cents: c.unit_price.cents(),
                state: contract_state_label(c.state).to_string(),
                delivered: c.delivered,
                missed: c.missed,
                deliveries: c.deliveries,
                start_tick: c.start_tick,
            })
            .collect();

        let event_start = world.journal.events.len().saturating_sub(EVENT_TAIL);
        let events = world
            .journal
            .events
            .iter()
            .skip(event_start)
            .map(|r| EventRow {
                seq: r.seq,
                tick: r.tick,
                kind: r.event.kind().to_string(),
                text: event_text(state, &r.event),
            })
            .collect();

        WorldSnapshot {
            tick,
            year: tick / DAYS_PER_YEAR + 1,
            day_of_year: tick % DAYS_PER_YEAR + 1,
            status: match &state.status {
                SimStatus::Running => "running".to_string(),
                SimStatus::Halted { reason } => format!("halted: {reason}"),
            },
            stats: Stats {
                population: state.agents.len() as u32,
                employed,
                unemployed,
                owners,
                hungry,
                money_total_cents: state.total_cash().cents(),
                food_price_cents: state.market.last_prices.get(&Good::Food).map(|p| p.cents()),
                food_stock: state.businesses.values().map(|b| b.stock(Good::Food)).sum(),
            },
            agents,
            businesses,
            markets,
            contracts,
            price_history: PriceHistory { ticks, series },
            events,
        }
    }
}

/// Human label for a contract's lifecycle state.
pub(crate) fn contract_state_label(s: crate::contracts::ContractState) -> &'static str {
    match s {
        crate::contracts::ContractState::Active => "active",
        crate::contracts::ContractState::Completed => "completed",
        crate::contracts::ContractState::Breached => "breached",
        crate::contracts::ContractState::Terminated => "terminated",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldConfig;

    #[test]
    fn snapshot_reflects_a_running_world() {
        let mut w = World::from_config(WorldConfig::default_with_seed(6));
        w.run_ticks(20).unwrap();
        let s = WorldSnapshot::capture(&w);
        assert_eq!(s.tick, 20);
        assert_eq!(s.year, 1);
        assert_eq!(s.day_of_year, 21);
        assert_eq!(s.stats.population, 29);
        assert_eq!(s.agents.len(), 29);
        assert_eq!(s.businesses.len(), 10);
        assert_eq!(
            s.price_history.series.len(),
            Good::ALL.len() - 1,
            "the home asset is not charted"
        );
        let farm = s
            .businesses
            .iter()
            .find(|b| b.kind == "farm")
            .expect("farms exist");
        assert!(
            farm.input_stock.iter().any(|r| r.good == "tools"),
            "tool users report tool stock"
        );
        for b in &s.businesses {
            assert!(b.books.revenue_cents >= 0);
            assert_eq!(
                b.books.assets_cents,
                b.cash_cents + b.books.inventory_value_cents,
                "balance sheet adds up"
            );
        }
        assert!(
            s.businesses.iter().any(|b| b.books.revenue_cents > 0),
            "someone sold something in 20 days"
        );
        assert_eq!(s.markets.len(), Good::ALL.len());
        let food = s.markets.iter().find(|m| m.good == "food").unwrap();
        assert!(food.world_stock > 0, "the town holds food");
        assert!(food.demand_qty > 0, "households always shop toward target");
        assert!(
            s.markets.iter().any(|m| m.sellers > 0),
            "someone is offering something"
        );
        assert_eq!(s.price_history.ticks.len(), 20);
        assert!(!s.events.is_empty());
        assert!(s.status == "running");
        // Snapshot must serialize cleanly for IPC.
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("price_history"));
    }
}
