//! Phase 1 integration: the construction chain houses the wealthy.
//!
//! Wood is felled, bricks are fired, the construction company assembles
//! them into homes, and households that crossed the home cash floor buy
//! one — moving large hoards back into the wage cycle — all under every
//! conservation invariant (goods conservation counts owned homes).

use sim_core::{BusinessKind, Good, Money, World, WorldConfig};

const RUN_DAYS: u64 = 450;

#[test]
fn wood_bricks_homes_reach_wealthy_households() {
    let mut w = World::from_config(WorldConfig::default_with_seed(42));
    w.run_ticks(RUN_DAYS).unwrap();

    let by_kind = |kind: BusinessKind| {
        w.state
            .businesses
            .values()
            .find(|b| b.kind == kind)
            .expect("worldgen builds every kind")
    };
    let cc = by_kind(BusinessKind::ConstructionCo);
    assert!(
        cc.books.revenue > Money::ZERO,
        "the construction company must sell homes"
    );
    assert!(
        cc.books.input_costs > Money::ZERO,
        "the construction company must buy wood and bricks"
    );
    assert!(
        by_kind(BusinessKind::LumberCamp).books.revenue > Money::ZERO,
        "the lumber camp must sell wood"
    );
    assert!(
        by_kind(BusinessKind::Brickworks).books.revenue > Money::ZERO,
        "the brickworks must sell bricks"
    );

    let homeowners = w.state.agents.values().filter(|a| a.owns_home).count();
    assert!(homeowners >= 1, "someone crossed the floor and bought");

    // Conservation to the end for every good, including homes held by
    // households (explicit here for release runs).
    for good in Good::ALL {
        assert_eq!(
            w.state.total_goods(good),
            w.state.expected_total_goods[&good],
            "goods reconciliation for {good}"
        );
    }
    assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    assert!(!w.is_halted());
}
