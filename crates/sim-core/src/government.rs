//! The town government (Phase 4): the fiscal kernel — one tax end to end,
//! and a budget that spends on something real.
//!
//! The government is a first-class ledger account like the bank, but unlike
//! the bank it is born broke: its treasury starts at zero and every cent it
//! spends was first collected as tax (the BRIEF's "cannot spend unlimited
//! money" — deficits and government debt are a later Phase 4 lever,
//! DECISIONS.md #029).
//!
//! **Sales tax** (the one tax, end to end): a seller-side levy of
//! `sales_tax_bp` on every goods sale — market trades and contract
//! deliveries alike, so contracting is never a tax dodge. It is collected at
//! the same ledger site that books the seller's revenue: the buyer pays the
//! posted price, the seller immediately remits `value × rate / 10000`
//! rounded toward zero, and the sub-cent remainder explicitly stays with
//! the seller. Exempt by design: bank liquidation fire-sales (distress
//! recoveries, not commerce), contract penalties (damages), wages,
//! dividends and business sales (income/capital taxation are separate
//! future levers). The player's `SetSalesTax` command moves the rate,
//! clamped to 0..=10_000 bp.
//!
//! **Welfare floor** (the budget's real spending): daily, in its own tick
//! phase between banking and consumption, every agent holding less cash
//! than `WELFARE_FLOOR` is topped up to the floor — most destitute first
//! (cash, then id), until the treasury runs dry. The floor is ~2 days of
//! food: the dole covers eating, nothing else. Payments are ordinary
//! ledger transfers; the `tax_reconciliation` invariant holds the books to
//! the cash every sweep.

use crate::events::Event;
use crate::goods::Good;
use crate::ids::{AccountId, AgentId, BusinessId};
use crate::ledger::{self, LedgerError, TxKind};
use crate::metrics::DayAccumulator;
use crate::money::Money;
use crate::world::{Journal, SimState};
use serde::{Deserialize, Serialize};

/// Sales tax applied at worldgen, in basis points of every goods sale
/// (100 = 1%). A seller-side turnover tax CASCADES down a production
/// chain — wheat, flour and food are each taxed, so the cumulative wedge
/// on food's final value is roughly three times the rate. Soak-calibrated
/// (DECISIONS.md #029): at 300 bp the wedge exceeded the food chain's
/// adaptation capacity and two of the four standing seeds starved to a
/// dead town by year 4; at 100 bp all four hold the baseline's 13
/// employed through the decade. The player can push the rate higher and
/// watch that tradeoff play out — the default world must survive it.
pub const DEFAULT_SALES_TAX_BP: i64 = 100;
/// The player command's clamp: 0..=100%.
pub const MAX_SALES_TAX_BP: i64 = 10_000;
/// The welfare floor at worldgen: agents are topped up to this daily,
/// treasury permitting — about two days of food at start prices.
/// `SetWelfareFloor` moves it (0..=`MAX_WELFARE_FLOOR`).
pub const WELFARE_FLOOR: Money = Money::from_cents(1_200);
/// Ceiling on the welfare-floor lever ($100/day — far past any sane
/// dole; bounds the command, not the imagination).
pub const MAX_WELFARE_FLOOR: Money = Money::from_cents(10_000);
/// The statutory minimum wage at worldgen — equal to the wage machinery's
/// long-standing mechanical floor, so the default changes nothing.
/// `SetMinimumWage` moves it within
/// `MIN_MINIMUM_WAGE..=MAX_MINIMUM_WAGE`; the statutory minimum can never
/// go below the mechanical floor.
pub const DEFAULT_MINIMUM_WAGE: Money = Money::from_cents(300);
pub const MIN_MINIMUM_WAGE: Money = DEFAULT_MINIMUM_WAGE;
/// Ceiling on the minimum-wage lever ($100/day).
pub const MAX_MINIMUM_WAGE: Money = Money::from_cents(10_000);
/// Ceiling on the deficit-limit lever ($100,000 — several times the whole
/// town's money supply; effectively "unlimited" while bounding overflow).
pub const MAX_DEFICIT_LIMIT: Money = Money::from_cents(10_000_000);

/// The government's lifetime books: cash must always equal what these imply
/// (the `tax_reconciliation` invariant).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovBooks {
    pub tax_collected: Money,
    pub welfare_paid: Money,
    /// Net monetary policy applied directly to the treasury (mint − burn).
    pub policy_net: Money,
    /// Sovereign principal drawn from the bank (Phase 4 deficit lever).
    pub debt_drawn: Money,
    /// Sovereign principal repaid.
    pub debt_repaid: Money,
    /// Sovereign interest paid in cash.
    pub debt_interest_paid: Money,
    /// Sovereign interest the treasury could not pay, rolled into the
    /// principal instead (no cash moved — the state does not default, its
    /// debt compounds).
    pub debt_capitalized: Money,
}

impl GovBooks {
    /// The treasury balance these books imply.
    pub fn expected_cash(&self) -> Money {
        self.tax_collected + self.policy_net + self.debt_drawn
            - self.welfare_paid
            - self.debt_repaid
            - self.debt_interest_paid
    }

    /// The debt balance these books imply.
    pub fn expected_debt(&self) -> Money {
        self.debt_drawn + self.debt_capitalized - self.debt_repaid
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Government {
    pub cash: Money,
    /// Sales tax in basis points, applied to every goods sale from the
    /// moment it is set (`SetSalesTax`).
    pub sales_tax_bp: i64,
    /// The welfare floor agents are topped up to daily (`SetWelfareFloor`).
    pub welfare_floor: Money,
    /// The statutory minimum wage (`SetMinimumWage`): the wage review's
    /// floor, and non-compliant posted wages are forced up to it.
    pub minimum_wage: Money,
    /// Sovereign principal outstanding, owed to the bank.
    pub debt: Money,
    /// The deficit lever (`SetDeficitLimit`): the treasury may borrow from
    /// the bank to cover welfare shortfalls while `debt` is under this.
    /// Zero (the default) is a balanced budget — no borrowing, ever.
    pub debt_limit: Money,
    /// Sub-cent sovereign interest carry, in milli-cents. Not money.
    pub debt_accrued_milli: i64,
    pub books: GovBooks,
}

impl Government {
    pub fn new() -> Government {
        Government {
            cash: Money::ZERO,
            sales_tax_bp: DEFAULT_SALES_TAX_BP,
            welfare_floor: WELFARE_FLOOR,
            minimum_wage: DEFAULT_MINIMUM_WAGE,
            debt: Money::ZERO,
            debt_limit: Money::ZERO,
            debt_accrued_milli: 0,
            books: GovBooks::default(),
        }
    }

    /// Today's sovereign interest accrual in milli-cents, at the bank's
    /// CURRENT base rate — sovereign debt floats, so the bank-rate lever
    /// prices the government's deficit too (360-day year, like loans).
    pub fn daily_debt_accrual_milli(&self, bank_rate_bp: i64) -> i64 {
        self.debt.cents() * bank_rate_bp / 3_600
    }
}

impl Default for Government {
    fn default() -> Self {
        Government::new()
    }
}

/// Collect sales tax on a seller's gross receipt of `value` for `good`:
/// the seller remits `value × rate / 10000` rounded toward zero (the
/// sub-cent remainder stays with the seller), booked on both sides at this
/// site. Returns the tax collected. Callers invoke this immediately after
/// the sale's own transfer, so the seller always holds at least the tax.
pub fn collect_sales_tax(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    seller: BusinessId,
    good: Good,
    value: Money,
) -> Result<Money, LedgerError> {
    let tax = value.mul_bp(state.government.sales_tax_bp);
    if tax <= Money::ZERO {
        return Ok(Money::ZERO);
    }
    ledger::transfer(
        state,
        journal,
        tick,
        AccountId::Business(seller),
        AccountId::Government,
        tax,
        TxKind::SalesTax { good },
    )?;
    state.government.books.tax_collected += tax;
    if let Some(b) = state.businesses.get_mut(&seller) {
        b.books.taxes_paid += tax;
        b.costs_window += tax;
    }
    Ok(tax)
}

/// Tick phase 8, the fiscal day, in a fixed order:
///
/// 1. **Sovereign interest** accrues on the debt at the bank's current
///    base rate (milli-cents; the carry is not money) and the whole cents
///    due are paid from the treasury — whatever cannot be paid is
///    CAPITALIZED into the principal (the state does not default, its
///    debt compounds).
/// 2. **Borrowing**: if the day's welfare bill exceeds the treasury and
///    the deficit lever allows it, the treasury draws the shortfall from
///    the bank — capped by the remaining `debt_limit` headroom and by the
///    bank's own liquidity floor (a drained bank rations the state like
///    any other borrower).
/// 3. **The welfare floor**: every agent below `welfare_floor` is topped
///    up to it, most destitute first (cash, then id), until the treasury
///    runs dry — the marginal recipient gets whatever is left.
/// 4. **Surplus retires principal**: any treasury left after the dole
///    pays the debt down — an indebted treasury never hoards.
pub fn run(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    acc: &mut DayAccumulator,
) -> Result<(), LedgerError> {
    // --- 1. Sovereign interest: accrue, pay what the treasury can,
    // capitalize the rest. ---
    if state.government.debt > Money::ZERO {
        let rate_bp = state.bank.base_rate_bp;
        state.government.debt_accrued_milli += state.government.daily_debt_accrual_milli(rate_bp);
        let due = Money::from_cents(state.government.debt_accrued_milli / 1_000);
        if due > Money::ZERO {
            let payable = due.min(state.government.cash);
            if payable > Money::ZERO {
                ledger::transfer(
                    state,
                    journal,
                    tick,
                    AccountId::Government,
                    AccountId::Bank,
                    payable,
                    TxKind::SovereignService { interest: true },
                )?;
                state.government.books.debt_interest_paid += payable;
                state.bank.books.sovereign_interest += payable;
            }
            let capitalized = due - payable;
            if capitalized > Money::ZERO {
                state.government.debt += capitalized;
                state.government.books.debt_capitalized += capitalized;
            }
            state.government.debt_accrued_milli -= due.cents() * 1_000;
        }
    }

    // --- 2. Borrow for the dole, deficit lever permitting. ---
    let floor = state.government.welfare_floor;
    let mut needy: Vec<(Money, AgentId)> = state
        .agents
        .values()
        .filter(|a| a.cash < floor)
        .map(|a| (a.cash, a.id))
        .collect();
    needy.sort();
    let bill: Money = needy.iter().map(|(cash, _)| floor - *cash).sum();
    let shortfall = (bill - state.government.cash).max(Money::ZERO);
    if shortfall > Money::ZERO && state.government.debt < state.government.debt_limit {
        let headroom = state.government.debt_limit - state.government.debt;
        let lendable = (state.bank.cash - state.bank.liquidity_floor()).max(Money::ZERO);
        let draw = shortfall.min(headroom).min(lendable);
        if draw > Money::ZERO {
            ledger::transfer(
                state,
                journal,
                tick,
                AccountId::Bank,
                AccountId::Government,
                draw,
                TxKind::SovereignDraw,
            )?;
            state.government.debt += draw;
            state.government.books.debt_drawn += draw;
            state.bank.books.sovereign_disbursed += draw;
            journal.push_event(
                tick,
                Event::GovBorrowed {
                    amount: draw,
                    outstanding: state.government.debt,
                },
            );
        }
    }

    // --- 3. The welfare floor. ---
    for (cash, aid) in needy {
        let treasury = state.government.cash;
        if treasury <= Money::ZERO {
            break;
        }
        let payment = (floor - cash).min(treasury);
        ledger::transfer(
            state,
            journal,
            tick,
            AccountId::Government,
            AccountId::Agent(aid),
            payment,
            TxKind::Welfare,
        )?;
        state.government.books.welfare_paid += payment;
        acc.welfare_recipients += 1;
        journal.push_event(
            tick,
            Event::WelfarePaid {
                agent: aid,
                amount: payment,
            },
        );
    }

    // --- 4. Surplus retires principal. ---
    if state.government.debt > Money::ZERO && state.government.cash > Money::ZERO {
        let repay = state.government.cash.min(state.government.debt);
        ledger::transfer(
            state,
            journal,
            tick,
            AccountId::Government,
            AccountId::Bank,
            repay,
            TxKind::SovereignService { interest: false },
        )?;
        state.government.debt -= repay;
        state.government.books.debt_repaid += repay;
        state.bank.books.sovereign_repaid += repay;
        if state.government.debt == Money::ZERO {
            // The residual sub-cent carry dies with the debt — it never
            // became money.
            state.government.debt_accrued_milli = 0;
            journal.push_event(tick, Event::GovDebtCleared);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldConfig;
    use crate::World;

    fn world() -> World {
        World::from_config(WorldConfig::default_with_seed(9))
    }

    fn fund_treasury(w: &mut World, cents: i64) {
        ledger::mint(
            &mut w.state,
            &mut w.journal,
            0,
            AccountId::Government,
            Money::from_cents(cents),
            "test treasury".into(),
        )
        .unwrap();
    }

    #[test]
    fn tax_rounds_toward_zero_and_the_remainder_stays_with_the_seller() {
        let mut w = world();
        // Pin the rate: the arithmetic below must not move with the
        // world-default calibration.
        w.state.government.sales_tax_bp = 300;
        let bid = *w.state.businesses.keys().next().unwrap();
        let seller_before = w.state.businesses[&bid].cash;
        // $1.00 at 3% → exactly 3¢ collected.
        collect_sales_tax(
            &mut w.state,
            &mut w.journal,
            1,
            bid,
            Good::Wheat,
            Money::from_cents(100),
        )
        .unwrap();
        assert_eq!(w.state.government.cash, Money::from_cents(3));
        assert_eq!(
            w.state.businesses[&bid].cash,
            seller_before - Money::from_cents(3)
        );
        // 33¢ at 3% → 0.99¢ rounds to zero: nothing moves, no journal entry.
        let txs_before = w.journal.transactions.len();
        collect_sales_tax(
            &mut w.state,
            &mut w.journal,
            1,
            bid,
            Good::Wheat,
            Money::from_cents(33),
        )
        .unwrap();
        assert_eq!(w.state.government.cash, Money::from_cents(3));
        assert_eq!(w.journal.transactions.len(), txs_before);
        assert_eq!(w.state.government.books.tax_collected, Money::from_cents(3));
        assert_eq!(
            w.state.businesses[&bid].books.taxes_paid,
            Money::from_cents(3)
        );
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn zero_rate_collects_nothing() {
        let mut w = world();
        let bid = *w.state.businesses.keys().next().unwrap();
        w.state.government.sales_tax_bp = 0;
        collect_sales_tax(
            &mut w.state,
            &mut w.journal,
            1,
            bid,
            Good::Wheat,
            Money::from_cents(100_000),
        )
        .unwrap();
        assert_eq!(w.state.government.cash, Money::ZERO);
        assert!(w.journal.transactions.is_empty());
    }

    #[test]
    fn welfare_tops_up_the_most_destitute_first_until_the_treasury_runs_dry() {
        let mut w = world();
        // Three poor agents: $2.00, $0.00, $5.00 (staged in id order, so
        // the payout order — cash ascending — differs from id order).
        let ids: Vec<AgentId> = w.state.agents.keys().copied().take(3).collect();
        let stage = [200i64, 0, 500];
        for (aid, cents) in ids.iter().zip(stage) {
            w.state.agents.get_mut(aid).unwrap().cash = Money::from_cents(cents);
        }
        // Everyone else far above the floor.
        for a in w.state.agents.values_mut() {
            if !ids.contains(&a.id) {
                a.cash = Money::from_cents(50_000);
            }
        }
        w.state.expected_total_money = w.state.total_cash();
        // Treasury covers the poorest fully ($12), the next partially.
        fund_treasury(&mut w, 1_800);
        let mut acc = DayAccumulator::default();
        run(&mut w.state, &mut w.journal, 1, &mut acc).unwrap();
        // ids[1] ($0.00) is poorest: topped to the floor.
        assert_eq!(w.state.agents[&ids[1]].cash, WELFARE_FLOOR);
        // ids[0] ($2.00) is next: gets the remaining $6.00.
        assert_eq!(w.state.agents[&ids[0]].cash, Money::from_cents(800));
        // ids[2] ($5.00): the treasury was dry.
        assert_eq!(w.state.agents[&ids[2]].cash, Money::from_cents(500));
        assert_eq!(w.state.government.cash, Money::ZERO);
        assert_eq!(acc.welfare_recipients, 2);
        assert_eq!(
            w.state.government.books.welfare_paid,
            Money::from_cents(1_800)
        );
        let payments = w
            .journal
            .events
            .iter()
            .filter(|e| matches!(e.event, Event::WelfarePaid { .. }))
            .count();
        assert_eq!(payments, 2);
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    /// Stage one destitute agent, everyone else comfortable, and open the
    /// deficit lever.
    fn stage_deficit(w: &mut World, limit_cents: i64) -> AgentId {
        let poor = *w.state.agents.keys().next().unwrap();
        for a in w.state.agents.values_mut() {
            a.cash = if a.id == poor {
                Money::ZERO
            } else {
                Money::from_cents(50_000)
            };
        }
        w.state.expected_total_money = w.state.total_cash();
        w.state.government.debt_limit = Money::from_cents(limit_cents);
        poor
    }

    #[test]
    fn the_deficit_lever_draws_services_and_retires_sovereign_debt() {
        let mut w = world();
        let poor = stage_deficit(&mut w, 50_000);
        let bank_before = w.state.bank.cash;
        let mut acc = DayAccumulator::default();
        // Day 1: empty treasury, a $12 dole bill — the treasury borrows.
        run(&mut w.state, &mut w.journal, 1, &mut acc).unwrap();
        assert_eq!(w.state.agents[&poor].cash, WELFARE_FLOOR, "dole on credit");
        assert_eq!(w.state.government.debt, WELFARE_FLOOR);
        assert_eq!(w.state.government.books.debt_drawn, WELFARE_FLOOR);
        assert_eq!(w.state.bank.books.sovereign_disbursed, WELFARE_FLOOR);
        assert_eq!(w.state.bank.cash, bank_before - WELFARE_FLOOR);
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(e.event, Event::GovBorrowed { .. })));
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
        // Fund the treasury; the surplus services interest then retires
        // the principal.
        ledger::mint(
            &mut w.state,
            &mut w.journal,
            2,
            AccountId::Government,
            Money::from_cents(5_000),
            "war chest".into(),
        )
        .unwrap();
        run(&mut w.state, &mut w.journal, 3, &mut acc).unwrap();
        assert_eq!(w.state.government.debt, Money::ZERO, "surplus retires debt");
        assert_eq!(w.state.government.debt_accrued_milli, 0);
        assert_eq!(w.state.government.books.debt_repaid, WELFARE_FLOOR);
        assert_eq!(w.state.bank.books.sovereign_repaid, WELFARE_FLOOR);
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(e.event, Event::GovDebtCleared)));
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn sovereign_interest_capitalizes_when_the_treasury_is_empty() {
        let mut w = world();
        stage_deficit(&mut w, 50_000);
        // A $100 floor makes one draw big enough for whole-cent daily
        // interest: $100.00 × 1,800 bp / 360 = 5¢/day.
        w.state.government.welfare_floor = Money::from_cents(10_000);
        let mut acc = DayAccumulator::default();
        run(&mut w.state, &mut w.journal, 1, &mut acc).unwrap();
        assert_eq!(w.state.government.debt, Money::from_cents(10_000));
        // Day 2: no intake, nothing to pay interest with — it compounds.
        run(&mut w.state, &mut w.journal, 2, &mut acc).unwrap();
        assert_eq!(
            w.state.government.debt,
            Money::from_cents(10_005),
            "unpayable interest capitalizes into the principal"
        );
        assert_eq!(
            w.state.government.books.debt_capitalized,
            Money::from_cents(5)
        );
        assert_eq!(
            w.state.government.debt,
            w.state.government.books.expected_debt()
        );
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn the_bank_rations_the_state_at_its_liquidity_floor() {
        let mut w = world();
        stage_deficit(&mut w, 1_000_000);
        // The bank holds only its floor plus $5 — the state gets $5, not
        // the whole bill.
        let floor = w.state.bank.liquidity_floor();
        let excess = w.state.bank.cash - floor - Money::from_cents(500);
        ledger::burn(
            &mut w.state,
            &mut w.journal,
            0,
            AccountId::Bank,
            excess,
            "stage illiquidity".into(),
        )
        .unwrap();
        let mut acc = DayAccumulator::default();
        run(&mut w.state, &mut w.journal, 1, &mut acc).unwrap();
        assert_eq!(
            w.state.government.debt,
            Money::from_cents(500),
            "the draw stops at the bank's liquidity floor"
        );
        assert_eq!(w.state.bank.cash, floor);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn agents_at_or_above_the_floor_get_nothing() {
        let mut w = world();
        for a in w.state.agents.values_mut() {
            a.cash = WELFARE_FLOOR;
        }
        w.state.expected_total_money = w.state.total_cash();
        fund_treasury(&mut w, 10_000);
        let mut acc = DayAccumulator::default();
        run(&mut w.state, &mut w.journal, 1, &mut acc).unwrap();
        assert_eq!(acc.welfare_recipients, 0);
        assert_eq!(w.state.government.cash, Money::from_cents(10_000));
        assert_eq!(w.state.government.books.welfare_paid, Money::ZERO);
    }
}
