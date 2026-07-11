//! Production phase: businesses run their recipe up to worker capacity,
//! input availability, and a produce-to-target inventory rule.

use crate::goods::Qty;
use crate::world::SimState;

/// Producers aim to hold this many days of expected sales in finished goods.
pub const OUTPUT_TARGET_DAYS: Qty = 4;

fn ceil_div(a: Qty, b: Qty) -> Qty {
    (a + b - 1) / b
}

pub fn run(state: &mut SimState) {
    for b in state.businesses.values_mut() {
        let expected = b.expected_daily_sales();
        let out_good = b.recipe.output.0;
        let out_per_batch = b.recipe.output.1.max(1);
        // Cover today's expected sales and refill toward the target buffer.
        let target = expected * OUTPUT_TARGET_DAYS + expected;
        let current = b.stock(out_good);
        if current >= target {
            continue;
        }
        let mut batches = ceil_div(target - current, out_per_batch).min(b.capacity_batches());
        for (good, per_batch) in &b.recipe.inputs {
            if *per_batch > 0 {
                batches = batches.min(b.stock(*good) / per_batch);
            }
        }
        if batches <= 0 {
            continue;
        }
        let inputs = b.recipe.inputs.clone();
        for (good, per_batch) in inputs {
            b.add_stock(good, -(per_batch * batches));
        }
        let out_qty = out_per_batch * batches;
        b.add_stock(out_good, out_qty);
        b.produced_today = out_qty;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goods::Good;
    use crate::worldgen::WorldConfig;
    use crate::World;

    #[test]
    fn production_consumes_inputs_and_respects_capacity() {
        let mut w = World::from_config(WorldConfig::default_with_seed(5));
        let mill_id = *w
            .state
            .businesses
            .values()
            .find(|b| b.sells == Good::Flour)
            .map(|b| &b.id)
            .unwrap();
        {
            let mill = w.state.businesses.get_mut(&mill_id).unwrap();
            mill.inventory.clear();
            mill.add_stock(Good::Wheat, 100);
            mill.sales_ema_milli = 1_000_000; // huge demand: capacity-bound
        }
        let capacity = w.state.businesses[&mill_id].capacity_batches();
        assert!(capacity > 0);
        run(&mut w.state);
        let mill = &w.state.businesses[&mill_id];
        assert_eq!(mill.produced_today, capacity); // 1 flour per batch
        assert_eq!(mill.stock(Good::Wheat), 100 - capacity);
        assert_eq!(mill.stock(Good::Flour), capacity);
    }

    #[test]
    fn production_is_input_bound_when_inputs_scarce() {
        let mut w = World::from_config(WorldConfig::default_with_seed(5));
        let mill_id = *w
            .state
            .businesses
            .values()
            .find(|b| b.sells == Good::Flour)
            .map(|b| &b.id)
            .unwrap();
        {
            let mill = w.state.businesses.get_mut(&mill_id).unwrap();
            mill.inventory.clear();
            mill.add_stock(Good::Wheat, 3);
            mill.sales_ema_milli = 1_000_000;
        }
        run(&mut w.state);
        let mill = &w.state.businesses[&mill_id];
        assert_eq!(mill.produced_today, 3);
        assert_eq!(mill.stock(Good::Wheat), 0);
    }

    #[test]
    fn production_stops_at_inventory_target() {
        let mut w = World::from_config(WorldConfig::default_with_seed(5));
        let farm_id = *w
            .state
            .businesses
            .values()
            .find(|b| b.sells == Good::Wheat)
            .map(|b| &b.id)
            .unwrap();
        {
            let farm = w.state.businesses.get_mut(&farm_id).unwrap();
            farm.sales_ema_milli = 2_000; // expects 2/day -> target 10
            farm.inventory.clear();
            farm.add_stock(Good::Wheat, 50);
        }
        run(&mut w.state);
        assert_eq!(w.state.businesses[&farm_id].produced_today, 0);
    }
}
