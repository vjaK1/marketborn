//! Continuously checked economic invariants.
//!
//! Runs every tick in debug builds and on the hash cadence in release builds.
//! A failure halts the simulation and produces a diagnostic report: tick,
//! invariant, expected vs actual, delta, and the last 50 transactions
//! touching the affected accounts.

use crate::goods::Good;
use crate::ids::AccountId;
use crate::ledger::Transaction;
use crate::world::{Journal, SimState};
use std::collections::BTreeSet;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvariantViolation {
    pub tick: u64,
    pub invariant: String,
    pub expected: String,
    pub actual: String,
    pub delta: String,
    pub recent_transactions: Vec<Transaction>,
}

impl InvariantViolation {
    pub fn summary(&self) -> String {
        format!(
            "invariant '{}' failed at tick {}: expected {}, actual {} (delta {})",
            self.invariant, self.tick, self.expected, self.actual, self.delta
        )
    }
}

impl fmt::Display for InvariantViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "INVARIANT FAILURE @ tick {}", self.tick)?;
        writeln!(f, "  invariant: {}", self.invariant)?;
        writeln!(f, "  expected:  {}", self.expected)?;
        writeln!(f, "  actual:    {}", self.actual)?;
        writeln!(f, "  delta:     {}", self.delta)?;
        writeln!(
            f,
            "  last {} transaction(s) touching the affected accounts:",
            self.recent_transactions.len()
        )?;
        for tx in &self.recent_transactions {
            let from = tx
                .from
                .map(|a| a.to_string())
                .unwrap_or_else(|| "MINT".into());
            let to = tx
                .to
                .map(|a| a.to_string())
                .unwrap_or_else(|| "BURN".into());
            writeln!(
                f,
                "    #{} t{}: {} -> {} {} ({:?})",
                tx.seq, tx.tick, from, to, tx.amount, tx.kind
            )?;
        }
        Ok(())
    }
}

const REPORT_TX_COUNT: usize = 50;

fn recent_txs(journal: &Journal, filter: Option<AccountId>) -> Vec<Transaction> {
    journal
        .transactions
        .iter()
        .rev()
        .filter(|tx| match filter {
            Some(acct) => tx.touches(acct),
            None => true,
        })
        .take(REPORT_TX_COUNT)
        .cloned()
        .collect()
}

fn violation(
    state: &SimState,
    journal: &Journal,
    invariant: &str,
    expected: impl fmt::Display,
    actual: impl fmt::Display,
    delta: impl fmt::Display,
    account: Option<AccountId>,
) -> Box<InvariantViolation> {
    Box::new(InvariantViolation {
        tick: state.tick,
        invariant: invariant.to_string(),
        expected: expected.to_string(),
        actual: actual.to_string(),
        delta: delta.to_string(),
        recent_transactions: recent_txs(journal, account),
    })
}

/// Run every invariant. Ok(()) or the first violation found.
pub fn check_all(state: &SimState, journal: &Journal) -> Result<(), Box<InvariantViolation>> {
    money_conservation(state, journal)?;
    non_negative_cash(state, journal)?;
    non_negative_inventory(state, journal)?;
    employment_reciprocity(state, journal)?;
    business_books(state, journal)?;
    contract_reconciliation(state, journal)?;
    debt_reconciliation(state, journal)?;
    tax_reconciliation(state, journal)?;
    // Aggregate reconciliation runs after the specific checks so a negative
    // stock reports as non_negative_inventory, not as a totals mismatch.
    goods_conservation(state, journal)?;
    Ok(())
}

/// Total money equals the expected total (adjusted only by explicit
/// monetary policy).
fn money_conservation(state: &SimState, journal: &Journal) -> Result<(), Box<InvariantViolation>> {
    let actual = state.total_cash();
    let expected = state.expected_total_money;
    if actual != expected {
        return Err(violation(
            state,
            journal,
            "money_conservation",
            expected,
            actual,
            actual - expected,
            None,
        ));
    }
    Ok(())
}

/// Per-business accounting reconciliation: the live cash balance must equal
/// what the lifetime books imply (starting cash + revenue + owner
/// investment + policy, minus inputs, tools, wages and dividends). A
/// mismatch means a cash flow reached the business without being
/// categorized at its ledger site (DECISIONS.md #016).
fn business_books(state: &SimState, journal: &Journal) -> Result<(), Box<InvariantViolation>> {
    for b in state.businesses.values() {
        let expected = b.books.expected_cash();
        if b.cash != expected {
            return Err(violation(
                state,
                journal,
                "business_books",
                format!("{} books imply {expected}", b.id),
                format!("cash is {}", b.cash),
                b.cash - expected,
                Some(AccountId::Business(b.id)),
            ));
        }
    }
    Ok(())
}

/// Per-contract reconciliation (Phase 3): every contract's counters,
/// schedule and money totals must agree with themselves and its terms. The
/// cash identity — `paid_total == unit_price × qty × delivered` — is
/// updated at the same settlement site as the transfer, so a delivery that
/// moved money without counting (or counted without moving) halts here or
/// in `business_books`.
fn contract_reconciliation(
    state: &SimState,
    journal: &Journal,
) -> Result<(), Box<InvariantViolation>> {
    use crate::contracts::{ContractState, BREACH_AFTER_MISSES};
    for c in state.contracts.values() {
        let fail = |what: &str, expected: String, actual: String| {
            Err(violation(
                state,
                journal,
                "contract_reconciliation",
                format!("{} {what}: {expected}", c.id),
                actual,
                "contract state inconsistent",
                Some(AccountId::Business(c.seller)),
            ))
        };
        if !state.businesses.contains_key(&c.seller) || !state.businesses.contains_key(&c.buyer) {
            return fail(
                "parties",
                "both businesses exist".into(),
                format!("seller {} buyer {}", c.seller, c.buyer),
            );
        }
        if c.qty <= 0 || c.every == 0 || c.deliveries == 0 || c.unit_price.is_negative() {
            return fail(
                "terms",
                "positive qty/cadence/deliveries, non-negative price".into(),
                format!(
                    "qty {} every {} deliveries {} price {}",
                    c.qty, c.every, c.deliveries, c.unit_price
                ),
            );
        }
        let settled = c.delivered + c.missed;
        if settled > c.deliveries || c.consecutive_misses > c.missed {
            return fail(
                "counters",
                format!("settled ≤ {}, run ≤ missed", c.deliveries),
                format!(
                    "delivered {} missed {} run {}",
                    c.delivered, c.missed, c.consecutive_misses
                ),
            );
        }
        match c.state {
            ContractState::Active => {
                if settled >= c.deliveries || c.consecutive_misses >= BREACH_AFTER_MISSES {
                    return fail(
                        "active state",
                        "unfinished schedule, unbroken run".into(),
                        format!("settled {settled}, run {}", c.consecutive_misses),
                    );
                }
                let aligned = c.start_tick + (u64::from(settled) + 1) * c.every;
                if c.next_due != aligned {
                    return fail(
                        "schedule",
                        format!("next_due {aligned}"),
                        format!("next_due {}", c.next_due),
                    );
                }
            }
            ContractState::Completed => {
                if settled != c.deliveries {
                    return fail(
                        "completed state",
                        format!("settled == {}", c.deliveries),
                        format!("settled {settled}"),
                    );
                }
            }
            ContractState::Breached => {
                if c.consecutive_misses != BREACH_AFTER_MISSES {
                    return fail(
                        "breached state",
                        format!("run == {BREACH_AFTER_MISSES}"),
                        format!("run {}", c.consecutive_misses),
                    );
                }
            }
            // Voluntary exit can happen at any point in the schedule; the
            // counter bounds above still apply.
            ContractState::Terminated => {}
        }
        if c.delivered_units < 0 || c.delivered_units > c.qty * i64::from(c.delivered) {
            return fail(
                "delivered units",
                format!("0 ≤ units ≤ ceiling × {} settled days", c.delivered),
                format!("units {}", c.delivered_units),
            );
        }
        let expected_paid = c
            .unit_price
            .checked_mul_qty(c.delivered_units)
            .unwrap_or(crate::money::Money::MAX);
        if c.paid_total != expected_paid {
            return fail(
                "payments",
                format!(
                    "paid {expected_paid} for {} delivered units",
                    c.delivered_units
                ),
                format!("paid {}", c.paid_total),
            );
        }
        // Each miss pays at most one full penalty; a voluntary termination
        // adds one more.
        let penalty_events = i64::from(c.missed) + i64::from(c.state == ContractState::Terminated);
        let penalty_ceiling = c
            .delivery_cost()
            .mul_bp(crate::contracts::CONTRACT_PENALTY_BP)
            .checked_mul_qty(penalty_events)
            .unwrap_or(crate::money::Money::MAX);
        if c.penalties_paid_total.is_negative() || c.penalties_paid_total > penalty_ceiling {
            return fail(
                "penalties",
                format!("0 ≤ penalties ≤ {penalty_ceiling} for {} misses", c.missed),
                format!("penalties {}", c.penalties_paid_total),
            );
        }
    }
    Ok(())
}

/// Bank and debt reconciliation (Phase 3): the bank's cash must equal what
/// its lifetime books imply, per-loan counters must agree with themselves,
/// and the loan book's totals must sum to the bank's aggregates — a loan
/// flow that bypassed its bookkeeping site halts here or in
/// `money_conservation`/`business_books`.
fn debt_reconciliation(state: &SimState, journal: &Journal) -> Result<(), Box<InvariantViolation>> {
    use crate::bank::{LoanState, DEFAULT_AFTER_MISSES};
    let bank = &state.bank;
    let expected = bank.books.expected_cash();
    if bank.cash != expected {
        return Err(violation(
            state,
            journal,
            "debt_reconciliation",
            format!("bank books imply {expected}"),
            format!("bank cash is {}", bank.cash),
            bank.cash - expected,
            Some(AccountId::Bank),
        ));
    }
    let mut disbursed = crate::money::Money::ZERO;
    let mut repaid = crate::money::Money::ZERO;
    let mut interest = crate::money::Money::ZERO;
    for l in bank.loans.values() {
        let fail = |what: &str, expected: String, actual: String| {
            Err(violation(
                state,
                journal,
                "debt_reconciliation",
                format!("{} {what}: {expected}", l.id),
                actual,
                "loan state inconsistent",
                Some(AccountId::Business(l.borrower)),
            ))
        };
        if !state.businesses.contains_key(&l.borrower) {
            return fail(
                "borrower",
                "business exists".into(),
                format!("{} not found", l.borrower),
            );
        }
        if l.principal <= crate::money::Money::ZERO
            || l.outstanding.is_negative()
            || l.accrued_interest_milli < 0
        {
            return fail(
                "terms",
                "positive principal, non-negative balances".into(),
                format!(
                    "principal {} outstanding {} accrued_milli {}",
                    l.principal, l.outstanding, l.accrued_interest_milli
                ),
            );
        }
        if l.outstanding != l.principal - l.principal_repaid_total {
            return fail(
                "balance",
                format!("outstanding == {}", l.principal - l.principal_repaid_total),
                format!("outstanding {}", l.outstanding),
            );
        }
        if l.consecutive_misses > l.missed_payments {
            return fail(
                "counters",
                "run ≤ missed".into(),
                format!("run {} missed {}", l.consecutive_misses, l.missed_payments),
            );
        }
        match l.state {
            LoanState::Active => {
                if l.consecutive_misses >= DEFAULT_AFTER_MISSES {
                    return fail(
                        "active state",
                        format!("run < {DEFAULT_AFTER_MISSES}"),
                        format!("run {}", l.consecutive_misses),
                    );
                }
            }
            LoanState::Repaid => {
                if l.outstanding != crate::money::Money::ZERO {
                    return fail(
                        "repaid state",
                        "outstanding == 0".into(),
                        format!("outstanding {}", l.outstanding),
                    );
                }
            }
            LoanState::Defaulted => {}
        }
        disbursed += l.principal;
        repaid += l.principal_repaid_total;
        interest += l.interest_paid_total;
    }
    if disbursed != bank.books.principal_disbursed
        || repaid != bank.books.principal_repaid
        || interest != bank.books.interest_received
    {
        return Err(violation(
            state,
            journal,
            "debt_reconciliation",
            format!(
                "loan book sums to disbursed {} repaid {} interest {}",
                bank.books.principal_disbursed,
                bank.books.principal_repaid,
                bank.books.interest_received
            ),
            format!("loans sum to disbursed {disbursed} repaid {repaid} interest {interest}"),
            "aggregate/loan-book mismatch",
            Some(AccountId::Bank),
        ));
    }
    for (good, qty) in &bank.inventory {
        if *qty < 0 {
            return Err(violation(
                state,
                journal,
                "debt_reconciliation",
                ">= 0",
                format!("bank holds {qty} {good}"),
                *qty,
                Some(AccountId::Bank),
            ));
        }
    }
    Ok(())
}

/// Government fiscal reconciliation (Phase 4): the treasury must equal what
/// the government's books imply (taxes − welfare + policy), every business's
/// remitted taxes must sum exactly to what the government collected (every
/// cent of tax has a payer who booked it), and the rate must sit inside the
/// command clamp. A tax flow that bypassed a bookkeeping site halts here or
/// in `business_books`/`money_conservation`. Note for test staging: the
/// remittance sum makes `Books::taxes_paid` globally load-bearing — any
/// scenario surgery that resyncs a business's books mid-run must carry
/// `taxes_paid` forward (re-basing `starting_cash` to keep the local cash
/// identity).
fn tax_reconciliation(state: &SimState, journal: &Journal) -> Result<(), Box<InvariantViolation>> {
    let gov = &state.government;
    let expected = gov.books.expected_cash();
    if gov.cash != expected {
        return Err(violation(
            state,
            journal,
            "tax_reconciliation",
            format!("government books imply {expected}"),
            format!("treasury is {}", gov.cash),
            gov.cash - expected,
            Some(AccountId::Government),
        ));
    }
    if gov.cash.is_negative()
        || gov.books.tax_collected.is_negative()
        || gov.books.welfare_paid.is_negative()
    {
        return Err(violation(
            state,
            journal,
            "tax_reconciliation",
            "non-negative treasury and fiscal totals",
            format!(
                "treasury {} collected {} welfare {}",
                gov.cash, gov.books.tax_collected, gov.books.welfare_paid
            ),
            "negative fiscal amount",
            Some(AccountId::Government),
        ));
    }
    if !(0..=crate::government::MAX_SALES_TAX_BP).contains(&gov.sales_tax_bp) {
        return Err(violation(
            state,
            journal,
            "tax_reconciliation",
            format!("0 ≤ rate ≤ {}", crate::government::MAX_SALES_TAX_BP),
            format!("sales tax {} bp", gov.sales_tax_bp),
            "rate outside the command clamp",
            Some(AccountId::Government),
        ));
    }
    let remitted: crate::money::Money = state.businesses.values().map(|b| b.books.taxes_paid).sum();
    if remitted != gov.books.tax_collected {
        return Err(violation(
            state,
            journal,
            "tax_reconciliation",
            format!("businesses remitted {remitted}"),
            format!("government collected {}", gov.books.tax_collected),
            gov.books.tax_collected - remitted,
            Some(AccountId::Government),
        ));
    }
    Ok(())
}

/// Per-good reconciliation: total on-hand quantity (business inventories +
/// pantries) equals the expected total, which only the goods ledger
/// (production mints, consumption/wear burns) may move. Trades are zero-sum;
/// a mismatch means some inventory was touched outside the doorway or a
/// trade lost a side.
fn goods_conservation(state: &SimState, journal: &Journal) -> Result<(), Box<InvariantViolation>> {
    for good in Good::ALL {
        let actual = state.total_goods(good);
        let expected = state.expected_total_goods.get(&good).copied().unwrap_or(0);
        if actual != expected {
            return Err(violation(
                state,
                journal,
                "goods_conservation",
                format!("{expected} {good}"),
                format!("{actual} {good}"),
                actual - expected,
                None,
            ));
        }
    }
    Ok(())
}

/// No account balance may be negative — spending unavailable funds is
/// structurally impossible through the ledger, so a negative balance means
/// something bypassed it.
fn non_negative_cash(state: &SimState, journal: &Journal) -> Result<(), Box<InvariantViolation>> {
    for a in state.agents.values() {
        if a.cash.is_negative() {
            return Err(violation(
                state,
                journal,
                "non_negative_cash",
                ">= $0.00",
                format!("{} has {}", a.id, a.cash),
                a.cash,
                Some(AccountId::Agent(a.id)),
            ));
        }
    }
    for b in state.businesses.values() {
        if b.cash.is_negative() {
            return Err(violation(
                state,
                journal,
                "non_negative_cash",
                ">= $0.00",
                format!("{} has {}", b.id, b.cash),
                b.cash,
                Some(AccountId::Business(b.id)),
            ));
        }
    }
    Ok(())
}

/// Inventory quantities never go negative.
fn non_negative_inventory(
    state: &SimState,
    journal: &Journal,
) -> Result<(), Box<InvariantViolation>> {
    for b in state.businesses.values() {
        for good in Good::ALL {
            let qty = b.stock(good);
            if qty < 0 {
                return Err(violation(
                    state,
                    journal,
                    "non_negative_inventory",
                    ">= 0",
                    format!("{} holds {} {}", b.id, qty, good),
                    qty,
                    Some(AccountId::Business(b.id)),
                ));
            }
        }
    }
    for a in state.agents.values() {
        if a.pantry < 0 {
            return Err(violation(
                state,
                journal,
                "non_negative_inventory",
                ">= 0",
                format!("{} pantry holds {}", a.id, a.pantry),
                a.pantry,
                Some(AccountId::Agent(a.id)),
            ));
        }
    }
    Ok(())
}

/// Employment links are mutually consistent: every listed worker points back
/// at the business, every employed agent is listed exactly once.
fn employment_reciprocity(
    state: &SimState,
    journal: &Journal,
) -> Result<(), Box<InvariantViolation>> {
    let mut seen = BTreeSet::new();
    for b in state.businesses.values() {
        for aid in &b.workers {
            if !seen.insert(*aid) {
                return Err(violation(
                    state,
                    journal,
                    "employment_reciprocity",
                    "each agent employed at most once",
                    format!("{aid} appears on multiple rosters"),
                    "duplicate roster entry",
                    Some(AccountId::Agent(*aid)),
                ));
            }
            match state.agents.get(aid) {
                Some(a) if a.employer == Some(b.id) => {}
                Some(a) => {
                    return Err(violation(
                        state,
                        journal,
                        "employment_reciprocity",
                        format!("{aid} employer = {}", b.id),
                        format!("{aid} employer = {:?}", a.employer),
                        "roster/employer mismatch",
                        Some(AccountId::Agent(*aid)),
                    ));
                }
                None => {
                    return Err(violation(
                        state,
                        journal,
                        "employment_reciprocity",
                        "roster references existing agents",
                        format!("{aid} not found"),
                        "dangling roster entry",
                        None,
                    ));
                }
            }
        }
    }
    for a in state.agents.values() {
        if let Some(bid) = a.employer {
            let listed = state
                .businesses
                .get(&bid)
                .is_some_and(|b| b.workers.contains(&a.id));
            if !listed {
                return Err(violation(
                    state,
                    journal,
                    "employment_reciprocity",
                    format!("{} listed on {} roster", a.id, bid),
                    "not listed".to_string(),
                    "employer/roster mismatch",
                    Some(AccountId::Agent(a.id)),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use crate::worldgen::WorldConfig;
    use crate::World;

    #[test]
    fn fresh_world_passes_all_invariants() {
        let w = World::from_config(WorldConfig::default_with_seed(3));
        check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn corrupted_cash_is_reported_with_context() {
        let mut w = World::from_config(WorldConfig::default_with_seed(3));
        let id = *w.state.agents.keys().next().unwrap();
        w.state.agents.get_mut(&id).unwrap().cash += Money::from_cents(123);
        let v = check_all(&w.state, &w.journal).unwrap_err();
        assert_eq!(v.invariant, "money_conservation");
        assert!(v.summary().contains("tick 0"));
        assert!(v.delta.contains("1.23"));
    }

    #[test]
    fn negative_inventory_is_caught() {
        let mut w = World::from_config(WorldConfig::default_with_seed(3));
        let id = *w.state.businesses.keys().next().unwrap();
        w.state
            .businesses
            .get_mut(&id)
            .unwrap()
            .add_stock(Good::Wheat, -1_000_000);
        let v = check_all(&w.state, &w.journal).unwrap_err();
        assert_eq!(v.invariant, "non_negative_inventory");
    }

    #[test]
    fn goods_created_outside_the_ledger_are_caught() {
        let mut w = World::from_config(WorldConfig::default_with_seed(3));
        let id = *w.state.businesses.keys().next().unwrap();
        // Positive out-of-band stock: no negative anywhere, so only the
        // per-good reconciliation can see it.
        w.state
            .businesses
            .get_mut(&id)
            .unwrap()
            .add_stock(Good::Steel, 5);
        let v = check_all(&w.state, &w.journal).unwrap_err();
        assert_eq!(v.invariant, "goods_conservation");
        assert!(v.summary().contains("steel"));
        assert!(v.delta.contains('5'));
    }

    #[test]
    fn uncategorized_business_cash_flows_are_caught() {
        let mut w = World::from_config(WorldConfig::default_with_seed(3));
        let id = *w.state.businesses.keys().next().unwrap();
        // Move money between two businesses' cash without touching books —
        // total money is conserved, so only the books reconciliation sees
        // it.
        let other = *w.state.businesses.keys().last().unwrap();
        w.state.businesses.get_mut(&id).unwrap().cash += Money::from_cents(500);
        w.state.businesses.get_mut(&other).unwrap().cash -= Money::from_cents(500);
        let v = check_all(&w.state, &w.journal).unwrap_err();
        assert_eq!(v.invariant, "business_books");
        assert!(v.delta.contains("5.00"));
    }

    #[test]
    fn contract_payment_drift_is_caught() {
        let mut w = World::from_config(WorldConfig::default_with_seed(3));
        let ids: Vec<_> = w.state.businesses.keys().copied().collect();
        let cid = crate::contracts::sign(
            &mut w.state,
            &mut w.journal,
            0,
            crate::contracts::SupplyTerms {
                seller: ids[0],
                buyer: ids[2],
                good: crate::goods::Good::Wheat,
                qty: 5,
                unit_price: Money::from_cents(400),
            },
        );
        check_all(&w.state, &w.journal).unwrap();
        // Units counted as delivered without their money moving (schedule
        // kept aligned so the payment identity is what fires).
        {
            let c = w.state.contracts.get_mut(&cid).unwrap();
            c.delivered = 1;
            c.delivered_units = 5;
            c.next_due = 2;
        }
        let v = check_all(&w.state, &w.journal).unwrap_err();
        assert_eq!(v.invariant, "contract_reconciliation");
        assert!(v.expected.contains("paid"), "{}", v.expected);
    }

    #[test]
    fn contract_schedule_misalignment_is_caught() {
        let mut w = World::from_config(WorldConfig::default_with_seed(3));
        let ids: Vec<_> = w.state.businesses.keys().copied().collect();
        let cid = crate::contracts::sign(
            &mut w.state,
            &mut w.journal,
            0,
            crate::contracts::SupplyTerms {
                seller: ids[0],
                buyer: ids[2],
                good: crate::goods::Good::Wheat,
                qty: 5,
                unit_price: Money::from_cents(400),
            },
        );
        w.state.contracts.get_mut(&cid).unwrap().next_due = 9; // should be 1
        let v = check_all(&w.state, &w.journal).unwrap_err();
        assert_eq!(v.invariant, "contract_reconciliation");
        assert!(v.expected.contains("next_due"));
    }

    #[test]
    fn treasury_drift_is_caught() {
        let mut w = World::from_config(WorldConfig::default_with_seed(3));
        // Books say the treasury is empty; the cash says otherwise — but
        // compensate an agent so money conservation stays green and only
        // the fiscal reconciliation can see it.
        let id = *w.state.agents.keys().next().unwrap();
        w.state.government.cash += Money::from_cents(250);
        w.state.agents.get_mut(&id).unwrap().cash -= Money::from_cents(250);
        let v = check_all(&w.state, &w.journal).unwrap_err();
        assert_eq!(v.invariant, "tax_reconciliation");
        assert!(v.delta.contains("2.50"));
    }

    #[test]
    fn tax_remittance_sum_mismatch_is_caught() {
        let mut w = World::from_config(WorldConfig::default_with_seed(3));
        // A business's books claim a remittance the government never saw.
        // Compensating input_costs keeps its own cash identity green, so
        // only the cross-side remittance sum can fire.
        let id = *w.state.businesses.keys().next().unwrap();
        let b = w.state.businesses.get_mut(&id).unwrap();
        b.books.taxes_paid += Money::from_cents(300);
        b.books.input_costs -= Money::from_cents(300);
        let v = check_all(&w.state, &w.journal).unwrap_err();
        assert_eq!(v.invariant, "tax_reconciliation");
        assert!(v.expected.contains("remitted"), "{}", v.expected);
    }

    #[test]
    fn pantry_edits_outside_the_ledger_are_caught() {
        let mut w = World::from_config(WorldConfig::default_with_seed(3));
        let id = *w.state.agents.keys().next().unwrap();
        w.state.agents.get_mut(&id).unwrap().pantry += 2;
        let v = check_all(&w.state, &w.journal).unwrap_err();
        assert_eq!(v.invariant, "goods_conservation");
        assert!(v.summary().contains("food"));
    }

    #[test]
    fn dangling_employment_is_caught() {
        let mut w = World::from_config(WorldConfig::default_with_seed(3));
        // Point an employed agent somewhere it isn't rostered.
        let (aid, bid) = {
            let a = w
                .state
                .agents
                .values()
                .find(|a| a.employer.is_some())
                .unwrap();
            (a.id, a.employer.unwrap())
        };
        let other = *w.state.businesses.keys().find(|b| **b != bid).unwrap();
        w.state.agents.get_mut(&aid).unwrap().employer = Some(other);
        let v = check_all(&w.state, &w.journal).unwrap_err();
        assert_eq!(v.invariant, "employment_reciprocity");
    }
}
