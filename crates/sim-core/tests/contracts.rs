//! Phase 3 acceptance (first criterion): a supply contract is negotiated
//! and fulfilled end to end — in a natural run, no staging. The buyer's
//! owner weighs the offer through the utility engine (the journaled
//! SupplyReview), deliveries settle in tick phase 6 with money through the
//! ledger, and at least one contract runs its whole schedule out.

use sim_core::decision::{ContractAction, DecisionDetail};
use sim_core::{ContractState, Event, Good, World, WorldConfig};

#[test]
fn supply_contract_negotiated_and_fulfilled_end_to_end() {
    let mut w = World::from_config(WorldConfig::default_with_seed(42));
    w.run_ticks(400).unwrap();

    let count =
        |pred: &dyn Fn(&Event) -> bool| w.journal.events.iter().filter(|e| pred(&e.event)).count();
    let signed = count(&|e| matches!(e, Event::ContractSigned { .. }));
    let delivered = count(&|e| matches!(e, Event::ContractDelivered { .. }));
    assert!(signed > 0, "no supply contract was ever signed in 400 days");
    assert!(delivered > 0, "contracts signed but nothing ever delivered");

    // The signing was a scored decision, not a scripted event.
    assert!(
        w.journal.decisions.iter().any(|d| matches!(
            &d.detail,
            DecisionDetail::SupplyReview {
                chosen: ContractAction::Sign,
                ..
            }
        )),
        "a signed contract must trace back to a journaled SupplyReview"
    );

    // At least one contract ran its full schedule out, and its money adds
    // up delivery by delivery.
    let completed: Vec<_> = w
        .state
        .contracts
        .values()
        .filter(|c| c.state == ContractState::Completed)
        .collect();
    assert!(
        !completed.is_empty(),
        "no contract completed its schedule in 400 days ({} signed)",
        signed
    );
    for c in &completed {
        assert_eq!(c.delivered + c.missed, c.deliveries);
        assert!(
            c.delivered_units > 0,
            "a completed contract delivered something"
        );
        assert_eq!(
            c.paid_total,
            c.unit_price.checked_mul_qty(c.delivered_units).unwrap(),
            "paid exactly the agreed price for every unit delivered"
        );
    }

    // Conservation holds to the end (every-tick invariant sweeps ran in
    // debug; assert the totals explicitly anyway).
    assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    for good in Good::ALL {
        assert_eq!(
            w.state.total_goods(good),
            w.state.expected_total_goods[&good],
            "goods reconciliation for {good}"
        );
    }
}
