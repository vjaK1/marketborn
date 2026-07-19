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
    Mine,
    SteelMill,
    ToolFactory,
}

impl BusinessKind {
    pub fn label(self) -> &'static str {
        match self {
            BusinessKind::Farm => "farm",
            BusinessKind::Mill => "mill",
            BusinessKind::Bakery => "bakery",
            BusinessKind::Mine => "mine",
            BusinessKind::SteelMill => "steel mill",
            BusinessKind::ToolFactory => "tool factory",
        }
    }
}

/// Extra batches an equipped worker runs, in basis points of
/// `batches_per_worker` (5000 = +50%). Sized to close the farms' structural
/// capacity gap without gluting the town's fixed food demand — a larger
/// bonus floods the wheat market and kills the farms it equips
/// (DECISIONS.md #013). The fractional-batch remainder rounds toward zero
/// at the business level and is simply not produced — no party receives it
/// (ECONOMIC_RULES.md §Production).
pub const TOOL_BONUS_BP: Qty = 5000;
/// Worker-days of use before one tool wears out and is destroyed. Short on
/// purpose: replacement demand is the industry chain's entire market, and
/// at ~7 equipped workers town-wide this life yields the ~1.2 tools/day the
/// chain needs to cover its wage bill (ECONOMIC_RULES.md §World parameters).
pub const TOOL_LIFE_WORKER_DAYS: i64 = 6;

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
    /// Extraction businesses (farms, mines) equip workers with tools for a
    /// production bonus. Tools live in `inventory` like any other good.
    pub uses_tools: bool,
    /// Worker-days of tool use accumulated toward the next tool wearing out
    /// (a tool breaks every `TOOL_LIFE_WORKER_DAYS`).
    pub tool_wear: i64,

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

    /// Workers holding a tool today (tool-using businesses only). One tool
    /// equips one worker; spares sit idle.
    pub fn equipped_workers(&self) -> Qty {
        if !self.uses_tools {
            return 0;
        }
        (self.workers.len() as Qty).min(self.stock(Good::Tools))
    }

    /// Total production batches the current workforce can run today: base
    /// capacity plus the tool bonus for equipped workers, rounded toward
    /// zero at the business level (the fraction is not produced).
    pub fn capacity_batches(&self) -> Qty {
        let base = self.workers.len() as Qty * self.recipe.batches_per_worker;
        let bonus =
            self.equipped_workers() * self.recipe.batches_per_worker * TOOL_BONUS_BP / 10_000;
        base + bonus
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
