//! The single doorway for creating and destroying goods.
//!
//! Trades move goods between holders (zero-sum — they never touch the
//! conservation targets). Production mints, consumption and tool wear burn,
//! and all of it must go through here so `expected_total_goods` stays
//! truthful and the `goods_conservation` invariant catches any bypass.
//!
//! Unlike the money ledger, these operations do not journal transactions:
//! goods creation/destruction is high-volume and already visible through
//! `produced_today` and the metrics stream. Validation (enough stock, enough
//! pantry) stays at the call sites, exactly as production pre-computes
//! input-limited batches; an unvalidated call is a bug that the
//! `non_negative_inventory` and `goods_conservation` invariants halt on.

use crate::goods::{Good, Qty};
use crate::ids::{AgentId, BusinessId};
use crate::world::SimState;

/// Production mints `qty` units of `good` into a business inventory.
pub fn produce(state: &mut SimState, business: BusinessId, good: Good, qty: Qty) {
    debug_assert!(qty >= 0);
    if let Some(b) = state.businesses.get_mut(&business) {
        b.add_stock(good, qty);
        *state.expected_total_goods.entry(good).or_insert(0) += qty;
    }
}

/// Consumption burns `qty` units of `good` from a business inventory
/// (recipe inputs, worn-out tools).
pub fn consume_stock(state: &mut SimState, business: BusinessId, good: Good, qty: Qty) {
    debug_assert!(qty >= 0);
    if let Some(b) = state.businesses.get_mut(&business) {
        b.add_stock(good, -qty);
        *state.expected_total_goods.entry(good).or_insert(0) -= qty;
    }
}

/// An agent eats `qty` food from the pantry.
pub fn consume_pantry(state: &mut SimState, agent: AgentId, qty: Qty) {
    debug_assert!(qty >= 0);
    if let Some(a) = state.agents.get_mut(&agent) {
        a.pantry -= qty;
        *state.expected_total_goods.entry(Good::Food).or_insert(0) -= qty;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldConfig;
    use crate::World;

    #[test]
    fn produce_and_consume_keep_expected_totals_in_sync() {
        let mut w = World::from_config(WorldConfig::default_with_seed(9));
        let bid = *w.state.businesses.keys().next().unwrap();
        let before = w.state.total_goods(Good::Wheat);
        assert_eq!(w.state.expected_total_goods[&Good::Wheat], before);

        produce(&mut w.state, bid, Good::Wheat, 7);
        assert_eq!(w.state.total_goods(Good::Wheat), before + 7);
        assert_eq!(w.state.expected_total_goods[&Good::Wheat], before + 7);

        consume_stock(&mut w.state, bid, Good::Wheat, 3);
        assert_eq!(w.state.total_goods(Good::Wheat), before + 4);
        assert_eq!(w.state.expected_total_goods[&Good::Wheat], before + 4);

        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn pantry_consumption_burns_food_from_the_target() {
        let mut w = World::from_config(WorldConfig::default_with_seed(9));
        let aid = *w.state.agents.keys().next().unwrap();
        let before = w.state.expected_total_goods[&Good::Food];
        consume_pantry(&mut w.state, aid, 1);
        assert_eq!(w.state.expected_total_goods[&Good::Food], before - 1);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }
}
