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
    LumberCamp,
    Brickworks,
    ConstructionCo,
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
            BusinessKind::LumberCamp => "lumber camp",
            BusinessKind::Brickworks => "brickworks",
            BusinessKind::ConstructionCo => "construction co",
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

/// Lifetime cash-basis books. Every business cash flow is categorized at
/// its ledger site (sales and purchases in the goods market, payroll in the
/// labor phase, dividends and owner investment in decisions, monetary
/// policy in the money ledger); the `business_books` invariant reconciles
/// them against the live cash balance every sweep, so a flow that bypasses
/// the books halts the simulation (DECISIONS.md #016).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Books {
    /// Cash at founding.
    pub starting_cash: Money,
    /// Goods sales received.
    pub revenue: Money,
    /// Recipe-input purchases paid.
    pub input_costs: Money,
    /// Tool (capital) purchases paid.
    pub tool_costs: Money,
    /// Wages paid.
    pub wages: Money,
    /// Dividends distributed to the owner.
    pub dividends: Money,
    /// Owner recapitalizations received.
    pub owner_invested: Money,
    /// Contract-miss penalties received from failing counterparties.
    pub penalties_received: Money,
    /// Contract-miss penalties paid to wronged counterparties.
    pub penalties_paid: Money,
    /// Loan principal received from the bank (Phase 3).
    pub loan_received: Money,
    /// Loan principal repaid to the bank.
    pub principal_repaid: Money,
    /// Loan interest paid to the bank.
    pub interest_paid: Money,
    /// Cash seized by the bank in foreclosure.
    pub seized_cash: Money,
    /// Sales tax remitted to the government on goods sales (Phase 4).
    pub taxes_paid: Money,
    /// Net monetary policy applied directly to this account (mint − burn;
    /// may be negative).
    pub policy_net: Money,
    /// Units lost to spoilage across all goods held — a physical
    /// write-down, not a cash flow (excluded from the cash identity).
    pub spoiled_units: Qty,
    /// Units seized by the bank in foreclosure — likewise physical.
    pub seized_units: Qty,
}

impl Books {
    pub fn new(starting_cash: Money) -> Books {
        Books {
            starting_cash,
            revenue: Money::ZERO,
            input_costs: Money::ZERO,
            tool_costs: Money::ZERO,
            wages: Money::ZERO,
            dividends: Money::ZERO,
            owner_invested: Money::ZERO,
            penalties_received: Money::ZERO,
            penalties_paid: Money::ZERO,
            loan_received: Money::ZERO,
            principal_repaid: Money::ZERO,
            interest_paid: Money::ZERO,
            seized_cash: Money::ZERO,
            taxes_paid: Money::ZERO,
            policy_net: Money::ZERO,
            spoiled_units: 0,
            seized_units: 0,
        }
    }

    /// The cash balance these books imply. Must equal the live balance.
    pub fn expected_cash(&self) -> Money {
        self.starting_cash
            + self.revenue
            + self.owner_invested
            + self.penalties_received
            + self.loan_received
            + self.policy_net
            - self.input_costs
            - self.tool_costs
            - self.wages
            - self.dividends
            - self.penalties_paid
            - self.principal_repaid
            - self.interest_paid
            - self.seized_cash
            - self.taxes_paid
    }

    /// Lifetime operating profit: revenue and penalty income minus all
    /// operating outflows (contract penalties, loan interest and sales tax
    /// are operating consequences). Distributions (dividends) and financing
    /// (owner investment, loan principal, policy) are excluded.
    pub fn lifetime_profit(&self) -> Money {
        self.revenue + self.penalties_received
            - self.input_costs
            - self.tool_costs
            - self.wages
            - self.penalties_paid
            - self.interest_paid
            - self.taxes_paid
    }
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
    /// Lifetime cash-basis accounting, reconciled against `cash` by the
    /// `business_books` invariant.
    pub books: Books,
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
    /// Consecutive completed review windows with zero revenue. Feeds the
    /// price-deadlock breaker (DECISIONS.md #022): one quiet week is
    /// normal duopoly alternation; three is a dead price.
    pub dry_windows: u32,
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

    /// Inventory valued at last market execution prices, falling back to
    /// this business's own posted price for its sold good, else zero.
    /// Integer and deterministic — used by takeover pricing (state logic)
    /// and the balance-sheet view.
    pub fn inventory_value(&self, last_prices: &BTreeMap<Good, Money>) -> Money {
        let mut total = Money::ZERO;
        for (good, qty) in &self.inventory {
            let unit = last_prices
                .get(good)
                .copied()
                .unwrap_or(if *good == self.sells {
                    self.price
                } else {
                    Money::ZERO
                });
            total += unit.checked_mul_qty(*qty).unwrap_or(Money::MAX);
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn books_cash_identity_and_profit() {
        let mut books = Books::new(Money::from_cents(120_000));
        assert_eq!(books.expected_cash(), Money::from_cents(120_000));
        books.revenue += Money::from_cents(5_000);
        books.input_costs += Money::from_cents(2_000);
        books.tool_costs += Money::from_cents(1_000);
        books.wages += Money::from_cents(700);
        books.dividends += Money::from_cents(300);
        books.owner_invested += Money::from_cents(400);
        books.penalties_received += Money::from_cents(250);
        books.penalties_paid += Money::from_cents(150);
        books.taxes_paid += Money::from_cents(120);
        books.policy_net -= Money::from_cents(100);
        assert_eq!(
            books.expected_cash(),
            Money::from_cents(
                120_000 + 5_000 - 2_000 - 1_000 - 700 - 300 + 400 + 250 - 150 - 120 - 100
            )
        );
        assert_eq!(
            books.lifetime_profit(),
            Money::from_cents(5_000 + 250 - 2_000 - 1_000 - 700 - 150 - 120),
            "profit is operating only: dividends/financing excluded"
        );
    }
}
