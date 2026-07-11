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
    pub price_history: PriceHistory,
    pub events: Vec<EventRow>,
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
}

#[derive(Clone, Debug, Serialize)]
pub struct InputStockRow {
    pub good: String,
    pub qty: i64,
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

fn agent_label(state: &SimState, id: AgentId) -> String {
    state
        .agents
        .get(&id)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn business_label(state: &SimState, id: BusinessId) -> String {
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
    }
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
                    hungry_streak: a.hungry_streak,
                    days_unemployed: a.days_unemployed,
                }
            })
            .collect();

        let businesses = state
            .businesses
            .values()
            .map(|b| BusinessRow {
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
                input_stock: b
                    .recipe
                    .inputs
                    .iter()
                    .map(|(g, _)| InputStockRow {
                        good: g.name().to_string(),
                        qty: b.stock(*g),
                    })
                    .collect(),
                last_window_profit_cents: b.last_window_profit.cents(),
                sold_today: b.sold_today,
                produced_today: b.produced_today,
            })
            .collect();

        let history_len = world.journal.metrics.len().min(HISTORY_DAYS);
        let skip = world.journal.metrics.len() - history_len;
        let window: Vec<_> = world.journal.metrics.iter().skip(skip).collect();
        let ticks = window.iter().map(|m| m.tick).collect();
        let series = Good::ALL
            .iter()
            .map(|good| GoodSeries {
                good: good.name().to_string(),
                points: window
                    .iter()
                    .map(|m| m.avg_price.get(good).copied().flatten().map(|p| p.cents()))
                    .collect(),
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
            price_history: PriceHistory { ticks, series },
            events,
        }
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
        assert_eq!(s.stats.population, 20);
        assert_eq!(s.agents.len(), 20);
        assert_eq!(s.businesses.len(), 4);
        assert_eq!(s.price_history.ticks.len(), 20);
        assert!(!s.events.is_empty());
        assert!(s.status == "running");
        // Snapshot must serialize cleanly for IPC.
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("price_history"));
    }
}
