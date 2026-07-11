//! Businesses: production, staffing, pricing and cash.

use crate::goods::{Good, Qty};
use crate::ids::{AgentId, BusinessId};
use crate::money::Money;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BusinessKind {
    Farm,
    Mill,
    Bakery,
}

impl BusinessKind {
    pub fn label(self) -> &'static str {
        match self {
            BusinessKind::Farm => "farm",
            BusinessKind::Mill => "mill",
            BusinessKind::Bakery => "bakery",
        }
    }
}

/// A production recipe: consume `inputs` per batch, emit `output` per batch.
/// Each worker can run `batches_per_worker` batches per day.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recipe {
    pub inputs: Vec<(Good, Qty)>,
    pub output: (Good, Qty),
    pub batches_per_worker: Qty,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Business {
    pub id: BusinessId,
    pub name: String,
    pub kind: BusinessKind,
    pub owner: AgentId,
    pub cash: Money,
    /// Employees in hire order. Firing removes from the back (last in,
    /// first out) — deterministic and documented.
    pub workers: Vec<AgentId>,
    /// Structural staffing target. Hiring never exceeds it in Phase 0.
    pub target_headcount: u32,
    /// Daily wage per worker.
    pub wage: Money,
    /// All goods on site, inputs and outputs alike.
    pub inventory: BTreeMap<Good, Qty>,
    /// The good this business sells.
    pub sells: Good,
    /// Posted unit sell price for `sells`.
    pub price: Money,
    pub recipe: Recipe,

    // --- Rolling statistics (integer milli-units; part of hashed state
    // because decisions read them) ---
    /// Exponential moving average of daily units sold, in 1/1000 units.
    pub sales_ema_milli: i64,
    /// Days within the current review window the business sold out while
    /// demand went unmet.
    pub stockout_days: u32,
    /// Days the business has had unfilled vacancies since the last review.
    pub vacancy_days: u32,
    /// Consecutive days payroll could not be fully met.
    pub missed_payroll_days: u32,
    /// Revenue and costs accumulated since the last review (7-day window).
    pub revenue_window: Money,
    pub costs_window: Money,
    /// Profit over the last completed review window (for UI and wage review).
    pub last_window_profit: Money,

    // --- Per-day scratch, reset at tick start ---
    pub sold_today: Qty,
    pub produced_today: Qty,
}

impl Business {
    pub fn stock(&self, good: Good) -> Qty {
        self.inventory.get(&good).copied().unwrap_or(0)
    }

    pub fn add_stock(&mut self, good: Good, qty: Qty) {
        *self.inventory.entry(good).or_insert(0) += qty;
    }

    /// Expected units sold per day, rounded up from the EMA (never below 1
    /// so a stalled business still plans a minimal batch).
    pub fn expected_daily_sales(&self) -> Qty {
        ((self.sales_ema_milli + 999) / 1000).max(1)
    }

    /// Total production batches the current workforce can run today.
    pub fn capacity_batches(&self) -> Qty {
        self.workers.len() as Qty * self.recipe.batches_per_worker
    }

    pub fn daily_payroll(&self) -> Money {
        self.wage
            .checked_mul_qty(self.workers.len() as i64)
            .unwrap_or(Money::MAX)
    }

    pub fn vacancies(&self) -> u32 {
        (self.target_headcount as usize).saturating_sub(self.workers.len()) as u32
    }
}
