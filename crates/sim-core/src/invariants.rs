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
