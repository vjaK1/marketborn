//! Phase 3 acceptance (second criterion): the default→foreclosure flow,
//! end to end through full ticks — a borrower with real debt stops being
//! able to service it, misses three days running, defaults, and the bank
//! forecloses: cash seized, collateral goods seized at market prices and
//! fire-sold back into the town, the shortfall written off against bank
//! equity — with every invariant green through the whole arc (every-tick
//! sweeps in debug builds).
//!
//! The distress is staged (the natural path is separately verified: seeds
//! 42/7 produce organic loans and defaults in-run — see PROGRESS.md), so
//! the test survives future recalibrations of the surrounding economy.

use sim_core::{
    bank::{self, LoanState, DEFAULT_AFTER_MISSES},
    business::Books,
    Event, Good, Money, World, WorldConfig,
};

#[test]
fn default_leads_to_foreclosure_and_the_books_survive() {
    let mut w = World::from_config(WorldConfig::default_with_seed(11));
    // Nobody rescues the mill: no takeover appetite, no bold borrowing
    // elsewhere to muddy the ledger trail.
    for a in w.state.agents.values_mut() {
        a.traits.ambition = 0;
        a.traits.risk_tolerance = 0;
    }
    // The mill borrows real working capital.
    let mill = *w
        .state
        .businesses
        .values()
        .find(|b| b.sells == Good::Flour)
        .map(|b| &b.id)
        .unwrap();
    bank::issue(
        &mut w.state,
        &mut w.journal,
        0,
        mill,
        Money::from_cents(30_000),
    )
    .unwrap();

    // Then its world collapses: no staff, no cash, a broke owner — but
    // wheat on the shelf the bank can reach. Books are kept consistent
    // with every manual edit so the invariant sweeps stay meaningful.
    let owner = w.state.businesses[&mill].owner;
    let staff: Vec<_> = w.state.businesses[&mill].workers.clone();
    for aid in staff {
        w.state.agents.get_mut(&aid).unwrap().employer = None;
    }
    {
        let b = w.state.businesses.get_mut(&mill).unwrap();
        b.workers.clear();
        // No vacancies either: otherwise the mill re-hires itself off its
        // flour-sale proceeds and dies empty-handed the ordinary way —
        // the staged flow needs it to remain a pure debtor whose input
        // buying piles up seizable wheat.
        b.target_headcount = 0;
        b.cash = Money::from_cents(2_500);
        // Books tell the staged story consistently: the principal came in
        // and was burned on inputs, leaving $25 in the till.
        b.books = Books::new(Money::from_cents(2_500));
        b.books.loan_received = Money::from_cents(30_000);
        b.books.input_costs = Money::from_cents(30_000);
    }
    w.state.agents.get_mut(&owner).unwrap().cash = Money::from_cents(500);
    // Collateral needs a market valuation: the staged default lands on
    // day ~9, before this world's first organic wheat trade, and the
    // bank cannot value unpriced goods (natural defaults happen months
    // in, with a full price board). Seed the last executed price.
    w.state
        .market
        .last_prices
        .insert(Good::Wheat, Money::from_cents(500));
    w.state.expected_total_money = w.state.total_cash();

    // Full ticks: the mill can pay a few days' service from its last
    // cash, then starts missing; three consecutive misses default it.
    w.run_ticks(30).unwrap();

    let loan = w.state.bank.loans.values().next().unwrap();
    assert_eq!(loan.state, LoanState::Defaulted, "the loan defaulted");
    assert_eq!(loan.consecutive_misses, DEFAULT_AFTER_MISSES);
    let missed = w
        .journal
        .events
        .iter()
        .filter(|e| matches!(e.event, Event::LoanPaymentMissed { .. }))
        .count();
    assert!(missed >= DEFAULT_AFTER_MISSES as usize);
    assert!(w
        .journal
        .events
        .iter()
        .any(|e| matches!(e.event, Event::LoanDefaulted { business, .. } if business == mill)));

    // Foreclosure: the bank seized cash and goods and wrote off the rest.
    let seizure = w
        .journal
        .events
        .iter()
        .find_map(|e| match e.event {
            Event::CollateralSeized {
                business,
                cash,
                goods_value,
                written_off,
                ..
            } if business == mill => Some((cash, goods_value, written_off)),
            _ => None,
        })
        .expect("foreclosure ran");
    let (cash_seized, goods_value, written_off) = seizure;
    assert!(
        goods_value > Money::ZERO,
        "the mill's wheat was reachable collateral"
    );
    assert!(
        cash_seized + goods_value + written_off > Money::ZERO,
        "the claim was settled one way or another"
    );
    assert!(
        w.state.businesses[&mill].books.seized_units > 0,
        "goods physically left the borrower"
    );

    // The seized wheat re-entered the economy through the fire sale (or
    // still sits with the bank awaiting a buyer) — never destroyed.
    assert_eq!(
        w.state.total_goods(Good::Wheat),
        w.state.expected_total_goods[&Good::Wheat],
        "goods conservation through seizure and liquidation"
    );
    assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    assert_eq!(
        w.state.bank.cash,
        w.state.bank.books.expected_cash(),
        "bank books reconcile through the whole flow"
    );
    // The stripped business survives as an ordinary moribund firm — the
    // takeover machinery's problem now, not the bank's.
    assert!(w.state.businesses.contains_key(&mill));
}
