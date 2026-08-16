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
/// The welfare floor: agents are topped up to this daily, treasury
/// permitting — about two days of food at start prices.
pub const WELFARE_FLOOR: Money = Money::from_cents(1_200);

/// The government's lifetime books: cash must always equal what these imply
/// (the `tax_reconciliation` invariant).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovBooks {
    pub tax_collected: Money,
    pub welfare_paid: Money,
    /// Net monetary policy applied directly to the treasury (mint − burn).
    pub policy_net: Money,
}

impl GovBooks {
    /// The treasury balance these books imply.
    pub fn expected_cash(&self) -> Money {
        self.tax_collected + self.policy_net - self.welfare_paid
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Government {
    pub cash: Money,
    /// Sales tax in basis points, applied to every goods sale from the
    /// moment it is set (`SetSalesTax`).
    pub sales_tax_bp: i64,
    pub books: GovBooks,
}

impl Government {
    pub fn new() -> Government {
        Government {
            cash: Money::ZERO,
            sales_tax_bp: DEFAULT_SALES_TAX_BP,
            books: GovBooks::default(),
        }
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

/// Tick phase 8: the welfare floor. Every agent below `WELFARE_FLOOR` is
/// topped up to it, most destitute first (cash, then id), until the
/// treasury runs dry — the marginal recipient gets whatever is left.
pub fn run(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    acc: &mut DayAccumulator,
) -> Result<(), LedgerError> {
    if state.government.cash <= Money::ZERO {
        return Ok(());
    }
    let mut needy: Vec<(Money, AgentId)> = state
        .agents
        .values()
        .filter(|a| a.cash < WELFARE_FLOOR)
        .map(|a| (a.cash, a.id))
        .collect();
    needy.sort();
    for (cash, aid) in needy {
        let treasury = state.government.cash;
        if treasury <= Money::ZERO {
            break;
        }
        let payment = (WELFARE_FLOOR - cash).min(treasury);
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
