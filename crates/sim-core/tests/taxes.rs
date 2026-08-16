//! Phase 4 government kernel: one tax end to end.
//!
//! The sales tax is collected at both revenue sites (market trades and
//! contract deliveries), the treasury pays the welfare floor, and the
//! whole fiscal loop reconciles — organically on a pinned seed, with every
//! debug-build tick running the full invariant suite.

use sim_core::commands::PlayerCommand;
use sim_core::government::WELFARE_FLOOR;
use sim_core::ids::AccountId;
use sim_core::ledger::TxKind;
use sim_core::money::Money;
use sim_core::worldgen::WorldConfig;
use sim_core::{Event, Good, World};

const SEED: u64 = 42;

/// Unstaged: taxation happens organically from day one, every cent has a
/// payer who booked it, and the transaction journal carries the levy.
#[test]
fn sales_tax_collects_organically_and_reconciles() {
    let mut w = World::from_config(WorldConfig::default_with_seed(SEED));
    w.run_ticks(200).unwrap();

    let gov = &w.state.government;
    assert!(
        gov.books.tax_collected > Money::ZERO,
        "200 days of markets must have paid tax"
    );
    let remitted: Money = w
        .state
        .businesses
        .values()
        .map(|b| b.books.taxes_paid)
        .sum();
    assert_eq!(
        remitted, gov.books.tax_collected,
        "every collected cent was remitted by some business"
    );
    assert_eq!(
        gov.cash,
        gov.books.expected_cash(),
        "the treasury matches its books"
    );
    assert!(
        w.journal
            .transactions
            .iter()
            .any(|tx| matches!(tx.kind, TxKind::SalesTax { .. })
                && tx.to == Some(AccountId::Government)),
        "the levy is visible in the transaction journal"
    );
    assert_eq!(w.state.total_cash(), w.state.expected_total_money);
}

/// Contract deliveries carry the same levy as spot trades — settle one
/// delivery in isolation (no market phase ran, so the delivery is the only
/// possible tax source) and check the collection against the gross payment.
#[test]
fn contract_deliveries_are_taxed_at_the_settlement_site() {
    let mut w = World::from_config(WorldConfig::default_with_seed(SEED));
    let farm = *w
        .state
        .businesses
        .values()
        .find(|b| b.sells == Good::Wheat)
        .map(|b| &b.id)
        .unwrap();
    let mill = *w
        .state
        .businesses
        .values()
        .find(|b| b.sells == Good::Flour)
        .map(|b| &b.id)
        .unwrap();
    let cid = sim_core::contracts::sign(
        &mut w.state,
        &mut w.journal,
        0,
        sim_core::contracts::SupplyTerms {
            seller: farm,
            buyer: mill,
            good: Good::Wheat,
            qty: 5,
            unit_price: Money::from_cents(400),
        },
    );
    let mut acc = sim_core::metrics::DayAccumulator::default();
    sim_core::contracts::settle(&mut w.state, &mut w.journal, 1, &mut acc).unwrap();

    let c = &w.state.contracts[&cid];
    assert!(c.delivered_units > 0, "the staged delivery must land");
    let rate_bp = w.state.government.sales_tax_bp;
    assert_eq!(
        w.state.government.cash,
        c.paid_total.mul_bp(rate_bp),
        "the treasury holds exactly the levy on the gross contract payment"
    );
    assert!(w.state.government.cash > Money::ZERO);
    assert_eq!(
        w.state.businesses[&farm].books.taxes_paid,
        w.state.government.cash
    );
    assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    sim_core::invariants::check_all(&w.state, &w.journal).unwrap();
}

/// The welfare floor, end to end through the command channel: a destitute
/// agent is topped up to exactly the floor in the government phase.
#[test]
fn the_welfare_floor_catches_a_destitute_agent() {
    let mut w = World::from_config(WorldConfig::default_with_seed(SEED));
    let aid = *w
        .state
        .agents
        .values()
        .find(|a| a.owns.is_none())
        .map(|a| &a.id)
        .unwrap();
    let cash = w.state.agents[&aid].cash;
    // Fund the treasury and impoverish the agent, both through commands.
    w.queue_command(
        1,
        PlayerCommand::AdjustMoneySupply {
            account: AccountId::Government,
            delta: Money::from_cents(5_000),
            memo: "relief fund".into(),
        },
    )
    .unwrap();
    w.queue_command(
        1,
        PlayerCommand::AdjustMoneySupply {
            account: AccountId::Agent(aid),
            delta: -(cash - Money::from_cents(100)),
            memo: "ruin".into(),
        },
    )
    .unwrap();
    w.tick().unwrap();

    // Whatever wage/groceries happened in between, the government phase
    // topped the agent to exactly the floor before consumption.
    assert_eq!(w.state.agents[&aid].cash, WELFARE_FLOOR);
    assert!(w
        .journal
        .events
        .iter()
        .any(|e| matches!(e.event, Event::WelfarePaid { agent, .. } if agent == aid)));
    assert!(w.state.government.books.welfare_paid > Money::ZERO);
    assert_eq!(w.state.total_cash(), w.state.expected_total_money);
}

/// The `SetSalesTax` lever: rate 0 stops collection cold; the clamp holds.
#[test]
fn set_sales_tax_reprices_collection_and_clamps() {
    let mut w = World::from_config(WorldConfig::default_with_seed(SEED));
    w.queue_command(50, PlayerCommand::SetSalesTax { rate_bp: 0 })
        .unwrap();
    w.run_ticks(50).unwrap();
    let collected_at_50 = w.state.government.books.tax_collected;
    assert!(collected_at_50 > Money::ZERO, "taxed until the cut");
    w.run_ticks(50).unwrap();
    assert_eq!(
        w.state.government.books.tax_collected, collected_at_50,
        "a zero rate collects nothing"
    );
    // The clamp: an absurd rate lands on the ceiling, and the event says so.
    w.queue_command(101, PlayerCommand::SetSalesTax { rate_bp: 99_999 })
        .unwrap();
    w.tick().unwrap();
    assert_eq!(
        w.state.government.sales_tax_bp,
        sim_core::government::MAX_SALES_TAX_BP
    );
    assert!(w.journal.events.iter().any(|e| matches!(
        e.event,
        Event::SalesTaxSet {
            new_bp: sim_core::government::MAX_SALES_TAX_BP,
            ..
        }
    )));
}
