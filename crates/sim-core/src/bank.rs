//! The town bank (Phase 3): business credit, daily service, default and
//! foreclosure.
//!
//! One bank, capitalized at worldgen (its capital is part of the money
//! supply and the conservation target). v1 lends its own equity to
//! businesses as working-capital term loans — deposits, household credit
//! and bank ownership arrive in later increments (DECISIONS.md #027).
//!
//! A loan fixes principal, an annual rate in basis points (360-day year)
//! and an 84-day term at issue. Daily, in tick phase 7 (loan-id order):
//! interest accrues in integer MILLI-cents on the outstanding principal
//! (sub-cent remainders carry in the accumulator — no money exists until
//! it is paid through the ledger), then the day's service — accrued whole
//! cents of interest plus the principal installment — is collected
//! interest-first. A day the full service cannot be collected is a missed
//! payment; `DEFAULT_AFTER_MISSES` consecutive misses default the loan and
//! trigger foreclosure: the bank seizes cash up to the debt, then
//! inventory goods at last market prices (whole units — seizure is lumpy,
//! and the overshoot of at most one unit is the borrower's loss), and
//! writes off the unrecovered rest against its equity. Seized goods sit in
//! the bank's inventory (counted by goods conservation) and are
//! fire-sold daily to the same deterministic buyer queue the goods market
//! builds, at last market prices.
//!
//! Credit assessment is deterministic and integer: no second concurrent
//! loan, no lending to a borrower with a defaulted loan on the book, a
//! liquidity floor the bank will not lend through, and the borrower must
//! pass an income test (day-one service within half its expected daily
//! revenue) or a coverage test (principal within 70% of assets). The
//! player's rate lever (`SetBankRate`) reprices FUTURE loans only —
//! `probe_rate_shock` guards this contraction channel.

use crate::events::Event;
use crate::goods::{Good, Qty};
use crate::ids::{AccountId, BusinessId, LoanId};
use crate::ledger::{self, LedgerError, TxKind};
use crate::metrics::DayAccumulator;
use crate::money::Money;
use crate::world::{Journal, SimState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Loan term in days (one quarter, matching the contract horizon).
pub const LOAN_TERM_DAYS: u32 = 84;
/// Annual interest rate on new loans at worldgen, in basis points
/// (360-day year): 18%/year ≈ 0.05%/day.
pub const DEFAULT_BASE_RATE_BP: i64 = 1_800;
/// Consecutive missed daily payments that default the loan.
pub const DEFAULT_AFTER_MISSES: u32 = 3;
/// Share of its starting capital the bank keeps liquid — it stops lending
/// rather than dip below this. Defaults eat equity, equity shrinks the
/// lendable pool: the credit-contraction channel the BRIEF asks for.
pub const BANK_LIQUIDITY_FLOOR_BP: i64 = 2_500;
/// Income test: day-one debt service may take at most this share of the
/// borrower's expected daily revenue.
pub const DEBT_SERVICE_SHARE_BP: i64 = 5_000;
/// Coverage test: principal may be at most this share of the borrower's
/// assets (cash + inventory at last prices).
pub const COLLATERAL_SHARE_BP: i64 = 7_000;
/// Smallest loan worth the paperwork.
pub const MIN_LOAN: Money = Money::from_cents(1_000);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoanState {
    Active,
    /// Principal fully repaid (any sub-cent interest remainder dies with
    /// the loan — it never became money).
    Repaid,
    /// Defaulted after consecutive misses; foreclosure has run.
    Defaulted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loan {
    pub id: LoanId,
    pub borrower: BusinessId,
    pub principal: Money,
    /// Annual rate in basis points, fixed at issue (360-day year).
    pub rate_bp: i64,
    pub term_days: u32,
    pub start_tick: u64,
    /// Principal still owed.
    pub outstanding: Money,
    /// Sub-cent interest accrual carry, in milli-cents. Not money.
    pub accrued_interest_milli: i64,
    pub interest_paid_total: Money,
    pub principal_repaid_total: Money,
    pub missed_payments: u32,
    pub consecutive_misses: u32,
    pub state: LoanState,
}

impl Loan {
    /// Today's interest accrual in milli-cents: outstanding × rate / 360
    /// per day, exact in integers (cents × bp / 3600 = milli-cents).
    pub fn daily_accrual_milli(&self) -> i64 {
        self.outstanding.cents() * self.rate_bp / 3_600
    }

    /// The principal installment due today: straight-line over the term,
    /// with the final scheduled day collecting whatever remains (the
    /// division remainder is assigned to the last payment).
    pub fn installment(&self, tick: u64) -> Money {
        let elapsed = tick.saturating_sub(self.start_tick);
        if elapsed >= u64::from(self.term_days) {
            return self.outstanding;
        }
        let per_day = Money::from_cents(self.principal.cents() / i64::from(self.term_days));
        per_day.min(self.outstanding)
    }

    /// The full service due today (interest cents that will have accrued
    /// by settlement, plus the installment). The market phase uses this
    /// exact projection to protect the payment in the borrower's budget —
    /// it must stay in lockstep with `run`'s collection.
    pub fn projected_due(&self, tick: u64) -> Money {
        let interest_cents = (self.accrued_interest_milli + self.daily_accrual_milli()) / 1_000;
        Money::from_cents(interest_cents) + self.installment(tick)
    }
}

/// The bank's own lifetime books: cash must always equal what these imply
/// (the `debt_reconciliation` invariant).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankBooks {
    pub starting_capital: Money,
    pub principal_disbursed: Money,
    pub principal_repaid: Money,
    pub interest_received: Money,
    pub seized_cash: Money,
    pub liquidation_proceeds: Money,
    /// Net monetary policy applied directly to the bank (mint − burn).
    pub policy_net: Money,
    /// Debt written off in foreclosures (no cash moved — the equity hole).
    pub written_off: Money,
    /// Sovereign principal disbursed to the government (Phase 4 deficit
    /// lever; the government's books mirror these three exactly).
    pub sovereign_disbursed: Money,
    /// Sovereign principal repaid by the government.
    pub sovereign_repaid: Money,
    /// Sovereign interest received.
    pub sovereign_interest: Money,
}

impl BankBooks {
    pub fn new(starting_capital: Money) -> BankBooks {
        BankBooks {
            starting_capital,
            principal_disbursed: Money::ZERO,
            principal_repaid: Money::ZERO,
            interest_received: Money::ZERO,
            seized_cash: Money::ZERO,
            liquidation_proceeds: Money::ZERO,
            policy_net: Money::ZERO,
            written_off: Money::ZERO,
            sovereign_disbursed: Money::ZERO,
            sovereign_repaid: Money::ZERO,
            sovereign_interest: Money::ZERO,
        }
    }

    /// The cash balance these books imply.
    pub fn expected_cash(&self) -> Money {
        self.starting_capital
            + self.principal_repaid
            + self.interest_received
            + self.seized_cash
            + self.liquidation_proceeds
            + self.policy_net
            + self.sovereign_repaid
            + self.sovereign_interest
            - self.principal_disbursed
            - self.sovereign_disbursed
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bank {
    pub cash: Money,
    /// Annual rate applied to NEW loans, in basis points. The player's
    /// `SetBankRate` lever moves it; existing loans keep their fixed rate.
    pub base_rate_bp: i64,
    pub loans: BTreeMap<LoanId, Loan>,
    pub next_loan_id: u32,
    /// Seized collateral awaiting liquidation. Counted by goods
    /// conservation like any other holder.
    pub inventory: BTreeMap<Good, Qty>,
    pub books: BankBooks,
}

impl Bank {
    pub fn new(capital: Money) -> Bank {
        Bank {
            cash: capital,
            base_rate_bp: DEFAULT_BASE_RATE_BP,
            loans: BTreeMap::new(),
            next_loan_id: 0,
            inventory: BTreeMap::new(),
            books: BankBooks::new(capital),
        }
    }

    pub fn stock(&self, good: Good) -> Qty {
        self.inventory.get(&good).copied().unwrap_or(0)
    }

    /// Cash the bank keeps liquid no matter what.
    pub fn liquidity_floor(&self) -> Money {
        self.books.starting_capital.mul_bp(BANK_LIQUIDITY_FLOOR_BP)
    }

    /// Outstanding principal across active loans.
    pub fn debt_outstanding(&self) -> Money {
        self.loans
            .values()
            .filter(|l| l.state == LoanState::Active)
            .map(|l| l.outstanding)
            .sum()
    }

    pub fn active_loan_of(&self, borrower: BusinessId) -> Option<&Loan> {
        self.loans
            .values()
            .find(|l| l.state == LoanState::Active && l.borrower == borrower)
    }

    pub fn has_defaulted_before(&self, borrower: BusinessId) -> bool {
        self.loans
            .values()
            .any(|l| l.state == LoanState::Defaulted && l.borrower == borrower)
    }
}

/// Why the bank said no. Surfaced in the borrower's decision record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreditRefusal {
    ExistingLoan,
    PriorDefault,
    BankIlliquid,
    FailsTests,
}

impl CreditRefusal {
    pub fn label(self) -> &'static str {
        match self {
            CreditRefusal::ExistingLoan => "already carries a loan",
            CreditRefusal::PriorDefault => "has defaulted before",
            CreditRefusal::BankIlliquid => "the bank is at its liquidity floor",
            CreditRefusal::FailsTests => "fails both the income and coverage tests",
        }
    }
}

/// Deterministic credit assessment for `principal` to `borrower` at the
/// current base rate. `Ok(())` means the bank would sign today.
pub fn assess(
    state: &SimState,
    borrower: BusinessId,
    principal: Money,
) -> Result<(), CreditRefusal> {
    let bank = &state.bank;
    if bank.active_loan_of(borrower).is_some() {
        return Err(CreditRefusal::ExistingLoan);
    }
    if bank.has_defaulted_before(borrower) {
        return Err(CreditRefusal::PriorDefault);
    }
    if bank.cash < principal + bank.liquidity_floor() {
        return Err(CreditRefusal::BankIlliquid);
    }
    let Some(b) = state.businesses.get(&borrower) else {
        return Err(CreditRefusal::FailsTests);
    };
    // Income test: day-one service (installment + first day's interest)
    // within half of expected daily revenue.
    let installment = principal.cents() / i64::from(LOAN_TERM_DAYS);
    let day_one_interest = principal.cents() * bank.base_rate_bp / 3_600 / 1_000;
    let service = Money::from_cents(installment + day_one_interest);
    let expected_revenue = b
        .price
        .checked_mul_qty(b.expected_daily_sales())
        .unwrap_or(Money::ZERO);
    let income_ok = service <= expected_revenue.mul_bp(DEBT_SERVICE_SHARE_BP);
    // Coverage test: principal within 70% of assets.
    let assets = b.cash + b.inventory_value(&state.market.last_prices);
    let coverage_ok = principal <= assets.mul_bp(COLLATERAL_SHARE_BP);
    if income_ok || coverage_ok {
        Ok(())
    } else {
        Err(CreditRefusal::FailsTests)
    }
}

/// Issue an assessed loan: disburse principal through the ledger and open
/// the book entry. Caller must have run `assess`.
pub fn issue(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    borrower: BusinessId,
    principal: Money,
) -> Result<LoanId, LedgerError> {
    let rate_bp = state.bank.base_rate_bp;
    let id = LoanId(state.bank.next_loan_id);
    ledger::transfer(
        state,
        journal,
        tick,
        AccountId::Bank,
        AccountId::Business(borrower),
        principal,
        TxKind::LoanDisbursed { loan: id },
    )?;
    state.bank.next_loan_id += 1;
    state.bank.books.principal_disbursed += principal;
    if let Some(b) = state.businesses.get_mut(&borrower) {
        b.books.loan_received += principal;
    }
    state.bank.loans.insert(
        id,
        Loan {
            id,
            borrower,
            principal,
            rate_bp,
            term_days: LOAN_TERM_DAYS,
            start_tick: tick,
            outstanding: principal,
            accrued_interest_milli: 0,
            interest_paid_total: Money::ZERO,
            principal_repaid_total: Money::ZERO,
            missed_payments: 0,
            consecutive_misses: 0,
            state: LoanState::Active,
        },
    );
    journal.push_event(
        tick,
        Event::LoanIssued {
            loan: id,
            business: borrower,
            principal,
            rate_bp,
        },
    );
    Ok(id)
}

/// Today's loan service owed by `borrower` — protected from its market
/// spending (like contract takes; `market::market_budget`).
pub fn payment_due_today(state: &SimState, borrower: BusinessId, tick: u64) -> Money {
    state
        .bank
        .loans
        .values()
        .filter(|l| l.state == LoanState::Active && l.borrower == borrower)
        .map(|l| l.projected_due(tick))
        .sum()
}

/// Tick phase 7: accrue and collect on every active loan in id order, then
/// fire-sell any seized inventory.
pub fn run(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    acc: &mut DayAccumulator,
) -> Result<(), LedgerError> {
    let due: Vec<LoanId> = state
        .bank
        .loans
        .iter()
        .filter(|(_, l)| l.state == LoanState::Active)
        .map(|(id, _)| *id)
        .collect();
    for lid in due {
        // Accrue, then read the day's bill.
        let (borrower, interest_due, installment) = {
            let Some(l) = state.bank.loans.get_mut(&lid) else {
                continue;
            };
            l.accrued_interest_milli += l.daily_accrual_milli();
            let interest_cents = l.accrued_interest_milli / 1_000;
            (
                l.borrower,
                Money::from_cents(interest_cents),
                l.installment(tick),
            )
        };
        let service = interest_due + installment;
        let cash = ledger::balance(state, AccountId::Business(borrower))?;

        if service == Money::ZERO {
            // Nothing owed today (fresh loan day-of-issue edge): not a miss.
            continue;
        }
        if cash >= service {
            // --- Full service: interest first, then principal. ---
            if interest_due > Money::ZERO {
                ledger::transfer(
                    state,
                    journal,
                    tick,
                    AccountId::Business(borrower),
                    AccountId::Bank,
                    interest_due,
                    TxKind::LoanPayment {
                        loan: lid,
                        interest: true,
                    },
                )?;
                state.bank.books.interest_received += interest_due;
                if let Some(b) = state.businesses.get_mut(&borrower) {
                    b.books.interest_paid += interest_due;
                    b.costs_window += interest_due;
                }
            }
            if installment > Money::ZERO {
                ledger::transfer(
                    state,
                    journal,
                    tick,
                    AccountId::Business(borrower),
                    AccountId::Bank,
                    installment,
                    TxKind::LoanPayment {
                        loan: lid,
                        interest: false,
                    },
                )?;
                state.bank.books.principal_repaid += installment;
                if let Some(b) = state.businesses.get_mut(&borrower) {
                    b.books.principal_repaid += installment;
                }
            }
            let repaid = {
                let Some(l) = state.bank.loans.get_mut(&lid) else {
                    continue;
                };
                l.accrued_interest_milli -= interest_due.cents() * 1_000;
                l.interest_paid_total += interest_due;
                l.outstanding -= installment;
                l.principal_repaid_total += installment;
                l.consecutive_misses = 0;
                if l.outstanding == Money::ZERO {
                    // The sub-cent accrual remainder dies with the loan —
                    // it never became money.
                    l.accrued_interest_milli = 0;
                    l.state = LoanState::Repaid;
                    true
                } else {
                    false
                }
            };
            if repaid {
                journal.push_event(
                    tick,
                    Event::LoanRepaid {
                        loan: lid,
                        business: borrower,
                    },
                );
            }
        } else {
            // --- Miss: the full service could not be met. Nothing partial
            // moves (the borrower's cash is protected for tomorrow's
            // attempt); the miss is counted and three in a row foreclose.
            let shortfall = service - cash;
            let defaulted = {
                let Some(l) = state.bank.loans.get_mut(&lid) else {
                    continue;
                };
                l.missed_payments += 1;
                l.consecutive_misses += 1;
                l.consecutive_misses >= DEFAULT_AFTER_MISSES
            };
            journal.push_event(
                tick,
                Event::LoanPaymentMissed {
                    loan: lid,
                    business: borrower,
                    shortfall,
                },
            );
            if defaulted {
                foreclose(state, journal, tick, lid, acc)?;
            }
        }
    }
    liquidate_seized(state, journal, tick, acc)?;
    Ok(())
}

/// Foreclosure: seize cash up to the debt, then goods at last market
/// prices, write off the rest, and close the loan as defaulted.
fn foreclose(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    lid: LoanId,
    acc: &mut DayAccumulator,
) -> Result<(), LedgerError> {
    let (borrower, mut debt) = {
        let Some(l) = state.bank.loans.get(&lid) else {
            return Ok(());
        };
        // The claim: outstanding principal plus accrued whole cents.
        (
            l.borrower,
            l.outstanding + Money::from_cents(l.accrued_interest_milli / 1_000),
        )
    };
    let outstanding_at_default = debt;
    // 1. Cash, up to the debt.
    let cash = ledger::balance(state, AccountId::Business(borrower))?;
    let cash_taken = cash.min(debt);
    if cash_taken > Money::ZERO {
        ledger::transfer(
            state,
            journal,
            tick,
            AccountId::Business(borrower),
            AccountId::Bank,
            cash_taken,
            TxKind::CollateralSeized { loan: lid },
        )?;
        state.bank.books.seized_cash += cash_taken;
        if let Some(b) = state.businesses.get_mut(&borrower) {
            b.books.seized_cash += cash_taken;
        }
        debt -= cash_taken;
    }
    // 2. Goods at last market prices, whole units, in canonical good
    // order. Seizure is lumpy: the last unit may overshoot the debt by
    // less than one unit's value — the borrower's loss, documented.
    let mut goods_value = Money::ZERO;
    if debt > Money::ZERO {
        for good in Good::ALL {
            if debt <= Money::ZERO {
                break;
            }
            let unit = state
                .market
                .last_prices
                .get(&good)
                .copied()
                .unwrap_or(Money::ZERO);
            if unit <= Money::ZERO {
                continue; // unpriced goods cannot settle debt
            }
            let held = state
                .businesses
                .get(&borrower)
                .map(|b| b.stock(good))
                .unwrap_or(0);
            if held == 0 {
                continue;
            }
            let needed = (debt.cents() + unit.cents() - 1) / unit.cents();
            let take = held.min(needed);
            if let Some(b) = state.businesses.get_mut(&borrower) {
                b.add_stock(good, -take);
                b.books.seized_units += take;
            }
            *state.bank.inventory.entry(good).or_insert(0) += take;
            let value = unit.checked_mul_qty(take).unwrap_or(Money::MAX);
            goods_value += value;
            debt = (debt - value).max(Money::ZERO);
        }
    }
    // 3. The rest is the bank's loss.
    let writeoff = debt;
    {
        let Some(l) = state.bank.loans.get_mut(&lid) else {
            return Ok(());
        };
        l.state = LoanState::Defaulted;
        l.accrued_interest_milli = 0;
    }
    state.bank.books.written_off += writeoff;
    acc.loan_defaults += 1;
    journal.push_event(
        tick,
        Event::LoanDefaulted {
            loan: lid,
            business: borrower,
            outstanding: outstanding_at_default,
        },
    );
    journal.push_event(
        tick,
        Event::CollateralSeized {
            loan: lid,
            business: borrower,
            cash: cash_taken,
            goods_value,
            written_off: writeoff,
        },
    );
    Ok(())
}

/// Fire-sell seized inventory to the market's own deterministic buyer
/// queue at last market prices. Retries daily until sold; unpriced goods
/// wait for a price.
fn liquidate_seized(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    acc: &mut DayAccumulator,
) -> Result<(), LedgerError> {
    for good in Good::ALL {
        let qty = state.bank.stock(good);
        if qty == 0 {
            continue;
        }
        let Some(unit) = state.market.last_prices.get(&good).copied() else {
            continue;
        };
        if unit <= Money::ZERO {
            continue;
        }
        let proceeds = crate::market::bank_liquidation(state, journal, tick, good, qty, unit)?;
        if proceeds > Money::ZERO {
            state.bank.books.liquidation_proceeds += proceeds;
        }
        let _ = acc;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldConfig;
    use crate::World;

    fn world() -> World {
        World::from_config(WorldConfig::default_with_seed(5))
    }

    fn first_biz(w: &World) -> BusinessId {
        *w.state.businesses.keys().next().unwrap()
    }

    #[test]
    fn interest_accrues_in_milli_cents_and_sub_cent_days_carry() {
        let mut w = world();
        let bid = first_biz(&w);
        w.state.bank.base_rate_bp = 1_800;
        issue(
            &mut w.state,
            &mut w.journal,
            0,
            bid,
            Money::from_cents(1_000), // $10 at 18%/yr → half a cent a day
        )
        .unwrap();
        let l = w.state.bank.loans.values().next().unwrap();
        assert_eq!(l.daily_accrual_milli(), 500);
        // Day one owes the installment only — the half-cent isn't money.
        assert_eq!(l.projected_due(1), Money::from_cents(1_000 / 84));
        let mut acc = DayAccumulator::default();
        run(&mut w.state, &mut w.journal, 1, &mut acc).unwrap();
        let l = w.state.bank.loans.values().next().unwrap();
        assert_eq!(
            l.accrued_interest_milli % 1_000,
            500,
            "half a cent carried in the accumulator"
        );
        assert_eq!(l.interest_paid_total, Money::ZERO);
        // The declining balance accrues just under a cent by day 2
        // (500 + 494 milli); day 3 crosses the whole-cent line.
        run(&mut w.state, &mut w.journal, 2, &mut acc).unwrap();
        run(&mut w.state, &mut w.journal, 3, &mut acc).unwrap();
        let l = w.state.bank.loans.values().next().unwrap();
        assert!(
            l.interest_paid_total >= Money::from_cents(1),
            "the carried sub-cents became a paid whole cent"
        );
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn full_term_service_repays_the_loan_with_interest() {
        let mut w = world();
        let bid = first_biz(&w);
        // Plenty of borrower cash: every payment lands.
        w.state.businesses.get_mut(&bid).unwrap().cash = Money::from_cents(1_000_000);
        {
            let b = w.state.businesses.get_mut(&bid).unwrap();
            b.books = crate::business::Books::new(b.cash);
        }
        w.state.expected_total_money = w.state.total_cash();
        let bank_before = w.state.bank.cash;
        issue(
            &mut w.state,
            &mut w.journal,
            0,
            bid,
            Money::from_cents(50_000),
        )
        .unwrap();
        let mut acc = DayAccumulator::default();
        for t in 1..=u64::from(LOAN_TERM_DAYS) {
            run(&mut w.state, &mut w.journal, t, &mut acc).unwrap();
        }
        let l = w.state.bank.loans.values().next().unwrap();
        assert_eq!(l.state, LoanState::Repaid);
        assert_eq!(l.outstanding, Money::ZERO);
        assert_eq!(l.principal_repaid_total, Money::from_cents(50_000));
        assert!(
            l.interest_paid_total > Money::ZERO,
            "the bank earned interest"
        );
        assert_eq!(
            w.state.bank.cash,
            bank_before + l.interest_paid_total,
            "principal round-tripped; interest is the bank's margin"
        );
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(e.event, Event::LoanRepaid { .. })));
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn three_consecutive_misses_default_and_foreclose() {
        let mut w = world();
        let bid = first_biz(&w);
        // Borrower with priced collateral on the shelf but no cash.
        w.state
            .market
            .last_prices
            .insert(Good::Wheat, Money::from_cents(500));
        issue(
            &mut w.state,
            &mut w.journal,
            0,
            bid,
            Money::from_cents(20_000),
        )
        .unwrap();
        {
            let b = w.state.businesses.get_mut(&bid).unwrap();
            // The disbursed cash "was spent": books stay consistent by
            // recording it as input costs.
            b.books.input_costs += b.cash;
            b.cash = Money::ZERO;
        }
        w.state.expected_total_money = w.state.total_cash();
        let mut acc = DayAccumulator::default();
        run(&mut w.state, &mut w.journal, 1, &mut acc).unwrap();
        run(&mut w.state, &mut w.journal, 2, &mut acc).unwrap();
        assert_eq!(
            w.state.bank.loans.values().next().unwrap().state,
            LoanState::Active
        );
        run(&mut w.state, &mut w.journal, 3, &mut acc).unwrap();
        let l = w.state.bank.loans.values().next().unwrap();
        assert_eq!(l.state, LoanState::Defaulted);
        assert_eq!(l.consecutive_misses, DEFAULT_AFTER_MISSES);
        // The farm's seeded wheat (15 @ $5.00 = $75) was seized toward the
        // $200 debt — and the same phase's fire sale may already have
        // moved it on to market buyers, so assert the seizure itself.
        assert_eq!(
            w.state.businesses[&bid].books.seized_units, 15,
            "collateral seized"
        );
        assert_eq!(w.state.businesses[&bid].stock(Good::Wheat), 0);
        assert!(w.state.bank.books.written_off > Money::ZERO, "equity hole");
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(e.event, Event::LoanDefaulted { .. })));
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(e.event, Event::CollateralSeized { .. })));
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn seized_goods_are_fire_sold_back_into_the_market() {
        let mut w = world();
        // Stage: the bank holds 5 wheat; the mill wants wheat.
        *w.state.bank.inventory.entry(Good::Wheat).or_insert(0) += 5;
        *w.state.expected_total_goods.entry(Good::Wheat).or_insert(0) += 5;
        w.state
            .market
            .last_prices
            .insert(Good::Wheat, Money::from_cents(400));
        let mill = *w
            .state
            .businesses
            .values()
            .find(|b| b.sells == Good::Flour)
            .map(|b| &b.id)
            .unwrap();
        w.state.businesses.get_mut(&mill).unwrap().inventory.clear();
        let bank_cash = w.state.bank.cash;
        let mut acc = DayAccumulator::default();
        run(&mut w.state, &mut w.journal, 1, &mut acc).unwrap();
        assert!(
            w.state.bank.stock(Good::Wheat) < 5,
            "the market absorbed seized goods"
        );
        assert!(w.state.bank.cash > bank_cash, "liquidation raised cash");
        assert_eq!(
            w.state.bank.cash,
            w.state.bank.books.expected_cash(),
            "bank books track the sale"
        );
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    }

    #[test]
    fn assessment_gates_refuse_correctly() {
        let mut w = world();
        let bid = first_biz(&w);
        // A healthy farm passes.
        assert!(assess(&w.state, bid, Money::from_cents(10_000)).is_ok());
        // Second concurrent loan refused.
        issue(
            &mut w.state,
            &mut w.journal,
            0,
            bid,
            Money::from_cents(10_000),
        )
        .unwrap();
        assert_eq!(
            assess(&w.state, bid, Money::from_cents(1_000)),
            Err(CreditRefusal::ExistingLoan)
        );
        // Prior default refused forever (v1 credit memory).
        w.state.bank.loans.values_mut().next().unwrap().state = LoanState::Defaulted;
        assert_eq!(
            assess(&w.state, bid, Money::from_cents(1_000)),
            Err(CreditRefusal::PriorDefault)
        );
        // The bank protects its liquidity floor.
        let other = *w.state.businesses.keys().nth(1).unwrap();
        let too_big = w.state.bank.cash;
        assert_eq!(
            assess(&w.state, other, too_big),
            Err(CreditRefusal::BankIlliquid)
        );
        // A dead business with no income and no assets fails both tests.
        {
            let b = w.state.businesses.get_mut(&other).unwrap();
            b.inventory.clear();
            b.cash = Money::ZERO;
            b.sales_ema_milli = 0;
            b.price = Money::from_cents(10);
        }
        assert_eq!(
            assess(&w.state, other, Money::from_cents(50_000)),
            Err(CreditRefusal::FailsTests)
        );
    }
}
