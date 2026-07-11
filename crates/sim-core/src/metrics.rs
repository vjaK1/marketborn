//! Daily metrics: derived, journaled, charted. Never read by simulation
//! logic — metrics are pure outputs.

use crate::goods::{Good, Qty};
use crate::money::Money;
use crate::world::SimState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Per-tick scratch accumulator threaded through the tick's phases.
#[derive(Clone, Debug, Default)]
pub struct DayAccumulator {
    pub trade_volume: BTreeMap<Good, Qty>,
    pub trade_value: BTreeMap<Good, Money>,
    pub unmet_demand: BTreeMap<Good, Qty>,
    pub food_consumed: Qty,
    pub hungry_agents: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricsDay {
    pub tick: u64,
    pub money_total: Money,
    pub household_cash: Money,
    pub business_cash: Money,
    pub employed: u32,
    pub unemployed: u32,
    pub owners: u32,
    pub hungry: u32,
    /// Volume-weighted average execution price, when the good traded.
    pub avg_price: BTreeMap<Good, Option<Money>>,
    pub volume: BTreeMap<Good, Qty>,
    pub unmet_demand: BTreeMap<Good, Qty>,
    pub food_consumed: Qty,
    pub food_produced: Qty,
}

pub fn capture(state: &SimState, acc: &DayAccumulator, tick: u64) -> MetricsDay {
    let household_cash: Money = state.agents.values().map(|a| a.cash).sum();
    let business_cash: Money = state.businesses.values().map(|b| b.cash).sum();
    let mut employed = 0u32;
    let mut unemployed = 0u32;
    let mut owners = 0u32;
    for a in state.agents.values() {
        if a.owns.is_some() {
            owners += 1;
        } else if a.employer.is_some() {
            employed += 1;
        } else {
            unemployed += 1;
        }
    }
    let mut avg_price = BTreeMap::new();
    let mut volume = BTreeMap::new();
    for good in Good::ALL {
        let vol = acc.trade_volume.get(&good).copied().unwrap_or(0);
        let avg = if vol > 0 {
            let value = acc.trade_value.get(&good).copied().unwrap_or(Money::ZERO);
            Some(Money::from_cents(value.cents() / vol))
        } else {
            None
        };
        avg_price.insert(good, avg);
        volume.insert(good, vol);
    }
    let food_produced = state
        .businesses
        .values()
        .filter(|b| b.sells == Good::Food)
        .map(|b| b.produced_today)
        .sum();
    MetricsDay {
        tick,
        money_total: household_cash + business_cash,
        household_cash,
        business_cash,
        employed,
        unemployed,
        owners,
        hungry: acc.hungry_agents,
        avg_price,
        volume,
        unmet_demand: acc.unmet_demand.clone(),
        food_consumed: acc.food_consumed,
        food_produced,
    }
}
