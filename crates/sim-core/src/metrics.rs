//! Daily metrics: derived, journaled, charted. Never read by simulation
//! logic — metrics are pure outputs.

use crate::goods::{Good, Qty};
use crate::ids::BusinessId;
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
    pub spoiled: BTreeMap<Good, Qty>,
    pub food_consumed: Qty,
    pub hungry_agents: u32,
    pub contract_deliveries: u32,
    pub contract_misses: u32,
    pub loan_defaults: u32,
    pub welfare_recipients: u32,
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
    pub spoiled: BTreeMap<Good, Qty>,
    pub food_consumed: Qty,
    pub food_produced: Qty,
    /// Supply contracts currently active.
    pub contracts_active: u32,
    /// Deliveries settled today.
    pub contract_deliveries: u32,
    /// Deliveries missed today.
    pub contract_misses: u32,
    /// The bank's cash on hand.
    pub bank_cash: Money,
    /// Outstanding principal across active loans.
    pub debt_outstanding: Money,
    /// Loans currently being serviced.
    pub loans_active: u32,
    /// Loans that defaulted today.
    pub loan_defaults: u32,
    /// The government's treasury balance.
    pub govt_cash: Money,
    /// Lifetime sales tax collected (cumulative — diff for the daily take).
    pub tax_collected: Money,
    /// Lifetime welfare paid out (cumulative).
    pub welfare_paid: Money,
    /// Agents topped up to the welfare floor today.
    pub welfare_recipients: u32,
    /// Per-business daily state, for time-series analysis and (later) the
    /// business inspector's historical charts.
    pub businesses: BTreeMap<BusinessId, BizDay>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BizDay {
    pub owner: u32,
    pub cash: Money,
    pub workers: u32,
    pub wage: Money,
    pub price: Money,
    pub sold: Qty,
    pub produced: Qty,
    pub stock: Qty,
}

pub fn capture(state: &SimState, acc: &DayAccumulator, tick: u64) -> MetricsDay {
    let household_cash: Money = state.agents.values().map(|a| a.cash).sum();
    let business_cash: Money = state.businesses.values().map(|b| b.cash).sum();
    let bank_cash = state.bank.cash;
    let govt_cash = state.government.cash;
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
    let businesses = state
        .businesses
        .values()
        .map(|b| {
            (
                b.id,
                BizDay {
                    owner: b.owner.0,
                    cash: b.cash,
                    workers: b.workers.len() as u32,
                    wage: b.wage,
                    price: b.price,
                    sold: b.sold_today,
                    produced: b.produced_today,
                    stock: b.stock(b.sells),
                },
            )
        })
        .collect();
    MetricsDay {
        tick,
        money_total: household_cash + business_cash + bank_cash + govt_cash,
        household_cash,
        business_cash,
        employed,
        unemployed,
        owners,
        hungry: acc.hungry_agents,
        avg_price,
        volume,
        unmet_demand: acc.unmet_demand.clone(),
        spoiled: acc.spoiled.clone(),
        food_consumed: acc.food_consumed,
        food_produced,
        contracts_active: state
            .contracts
            .values()
            .filter(|c| c.state == crate::contracts::ContractState::Active)
            .count() as u32,
        contract_deliveries: acc.contract_deliveries,
        contract_misses: acc.contract_misses,
        bank_cash,
        debt_outstanding: state.bank.debt_outstanding(),
        loans_active: state
            .bank
            .loans
            .values()
            .filter(|l| l.state == crate::bank::LoanState::Active)
            .count() as u32,
        loan_defaults: acc.loan_defaults,
        govt_cash,
        tax_collected: state.government.books.tax_collected,
        welfare_paid: state.government.books.welfare_paid,
        welfare_recipients: acc.welfare_recipients,
        businesses,
    }
}
