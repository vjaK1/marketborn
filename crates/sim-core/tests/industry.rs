//! Phase 1 integration: the industry chain feeds farm productivity.
//!
//! The acceptance centerpiece from PLAN.md: ore is mined, smelted into
//! steel, forged into tools; farms and the mine buy tools and demonstrably
//! produce above their bare-handed capacity; tools wear out, sustaining
//! replacement demand — all under every conservation invariant.

use sim_core::{AccountId, BusinessId, BusinessKind, Good, TxKind, World, WorldConfig};

const RUN_DAYS: u64 = 180;

#[test]
fn ore_steel_tools_farm_productivity_chain() {
    let mut w = World::from_config(WorldConfig::default_with_seed(42));

    let extraction_ids: Vec<BusinessId> = w
        .state
        .businesses
        .values()
        .filter(|b| matches!(b.kind, BusinessKind::Farm | BusinessKind::Mine))
        .map(|b| b.id)
        .collect();
    assert_eq!(extraction_ids.len(), 3, "two farms and a mine");

    let mut saw_bonus_production = false;
    let mut saw_equipped_farm = false;
    for _ in 0..RUN_DAYS {
        w.tick().unwrap();
        for bid in &extraction_ids {
            let b = &w.state.businesses[bid];
            if b.equipped_workers() > 0 {
                saw_equipped_farm = true;
            }
            let base_capacity =
                b.workers.len() as i64 * b.recipe.batches_per_worker * b.recipe.output.1;
            if !b.workers.is_empty() && b.produced_today > base_capacity {
                saw_bonus_production = true;
            }
        }
    }

    // --- The chain traded at every stage (the transaction ring easily
    // holds this run; every purchase is journaled). ---
    let purchased_by = |good: Good, buyer: AccountId| -> i64 {
        w.journal
            .transactions
            .iter()
            .filter_map(|tx| match &tx.kind {
                TxKind::GoodsPurchase { good: g, qty, .. }
                    if *g == good && tx.from == Some(buyer) =>
                {
                    Some(*qty)
                }
                _ => None,
            })
            .sum()
    };
    let by_kind = |kind: BusinessKind| -> Vec<AccountId> {
        w.state
            .businesses
            .values()
            .filter(|b| b.kind == kind)
            .map(|b| AccountId::Business(b.id))
            .collect()
    };

    let ore_bought: i64 = by_kind(BusinessKind::SteelMill)
        .iter()
        .map(|a| purchased_by(Good::IronOre, *a))
        .sum();
    let steel_bought: i64 = by_kind(BusinessKind::ToolFactory)
        .iter()
        .map(|a| purchased_by(Good::Steel, *a))
        .sum();
    let tools_bought: i64 = extraction_ids
        .iter()
        .map(|id| purchased_by(Good::Tools, AccountId::Business(*id)))
        .sum();
    assert!(ore_bought > 0, "the steelworks must buy ore from the mine");
    assert!(steel_bought > 0, "the tool factory must buy steel");
    assert!(tools_bought > 0, "farms/mine must buy tools");

    // --- Tools reached the fields and raised output above bare-handed
    // capacity on at least one real production day. ---
    assert!(saw_equipped_farm, "some extraction business held tools");
    assert!(
        saw_bonus_production,
        "equipped workers must out-produce base capacity"
    );

    // --- Tools wear out: more tools were bought by users than they still
    // hold, so the difference was destroyed by use (replacement demand). ---
    let tools_held: i64 = extraction_ids
        .iter()
        .map(|id| w.state.businesses[id].stock(Good::Tools))
        .sum();
    assert!(
        tools_bought > tools_held,
        "wear must destroy tools over {RUN_DAYS} days (bought {tools_bought}, held {tools_held})"
    );

    // --- Conservation held to the end for every good (also enforced every
    // tick in debug builds; explicit here for release runs). ---
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
