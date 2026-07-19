//! Phase 1 acceptance: a 100-agent, 20-business world runs one sim year
//! headless with all invariants green. Debug builds sweep every invariant
//! every tick, so `run_ticks` succeeding IS the invariant guarantee; the
//! liveness asserts confirm the economy did not degenerate to a standstill.

use sim_core::{Good, World, WorldConfig};

#[test]
fn hundred_agent_twenty_business_year_is_green() {
    let mut w = World::from_config(WorldConfig {
        master_seed: 42,
        population: 100,
        hash_every: 50,
    });
    assert_eq!(w.state.agents.len(), 100);
    assert_eq!(w.state.businesses.len(), 20);

    w.run_ticks(365).unwrap();
    assert!(!w.is_halted());

    let employed = w
        .state
        .agents
        .values()
        .filter(|a| a.employer.is_some())
        .count();
    assert!(employed > 0, "the town still employs people");
    let recent_meals: i64 = w
        .journal
        .metrics
        .iter()
        .rev()
        .take(30)
        .map(|m| m.food_consumed)
        .sum();
    assert!(recent_meals > 0, "people still eat in month twelve");

    for good in Good::ALL {
        assert_eq!(
            w.state.total_goods(good),
            w.state.expected_total_goods[&good],
            "goods reconciliation for {good}"
        );
    }
    assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    for b in w.state.businesses.values() {
        assert_eq!(
            b.cash,
            b.books.expected_cash(),
            "{} books reconcile",
            b.name
        );
    }
}
