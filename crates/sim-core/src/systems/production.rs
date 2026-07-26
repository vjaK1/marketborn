//! Production phase: businesses run their recipe up to worker capacity,
//! input availability, and a produce-to-target inventory rule. Tool-using
//! businesses get a capacity bonus from equipped workers, and equipped
//! workers wear their tools down on any day the business produces.

use crate::business::TOOL_LIFE_WORKER_DAYS;
use crate::goods::{Good, Qty};
use crate::goods_ledger;
use crate::ids::BusinessId;
use crate::world::SimState;

/// Producers aim to hold this many days of expected sales in finished
/// goods (the produced buffer is this plus today's sales). The total
/// buffer (5 days) must sit strictly inside the light-glut threshold
/// (6 days) — a buffer equal to the glut line makes every healthy
/// producer flirt with weekly price cuts and slowly deflate itself to
/// the floor (DECISIONS.md #015).
pub const OUTPUT_TARGET_DAYS: Qty = 4;
/// Perishable producers hold a smaller buffer — a warehouse of bread is a
/// warehouse of rot. With 4%/day spoilage, a durable-sized buffer
/// equilibrates at a stock whose daily rot eats the whole margin. But the
/// buffer must still cover the upstream supply oscillation (~3 days from
/// input-batch buying): a 1-day larder gave the town a recurring
/// empty-shelf heartbeat that eventually killed the mill
/// (DECISIONS.md #015).
pub const PERISHABLE_TARGET_DAYS: Qty = 2;

fn ceil_div(a: Qty, b: Qty) -> Qty {
    (a + b - 1) / b
}

struct BatchPlan {
    batches: Qty,
    inputs: Vec<(Good, Qty)>,
    out_good: Good,
    out_per_batch: Qty,
    equipped: Qty,
}

pub fn run(state: &mut SimState) {
    let ids: Vec<BusinessId> = state.businesses.keys().copied().collect();
    for bid in ids {
        let plan: Option<BatchPlan> = {
            let Some(b) = state.businesses.get(&bid) else {
                continue;
            };
            // Plan for demand, not just trailing sales: recent stockout
            // days add to the rate one for one (the same demand-pull that
            // drives input orders — market::daily_input_need).
            let expected = b.expected_daily_sales() + b.stockout_days as Qty;
            let out_good = b.recipe.output.0;
            let out_per_batch = b.recipe.output.1.max(1);
            // Cover today's expected sales and refill toward the target
            // buffer (smaller for perishables).
            let buffer_days = if out_good.is_perishable() {
                PERISHABLE_TARGET_DAYS
            } else {
                OUTPUT_TARGET_DAYS
            };
            // Contract commitments stack on top: the seller accumulates its
            // rolling per-period promise across the week so the delivery is
            // on hand when settlement calls (Phase 3). The EMA alone cannot
            // plan for it — it only sees a delivery after it happens.
            let committed = crate::contracts::committed_per_period(state, bid, out_good);
            let target = expected * buffer_days + expected + committed;
            let current = b.stock(out_good);
            if current >= target {
                None
            } else {
                let mut batches =
                    ceil_div(target - current, out_per_batch).min(b.capacity_batches());
                for (good, per_batch) in &b.recipe.inputs {
                    if *per_batch > 0 {
                        batches = batches.min(b.stock(*good) / per_batch);
                    }
                }
                if batches <= 0 {
                    None
                } else {
                    Some(BatchPlan {
                        batches,
                        inputs: b.recipe.inputs.clone(),
                        out_good,
                        out_per_batch,
                        equipped: b.equipped_workers(),
                    })
                }
            }
        };
        let Some(plan) = plan else {
            continue;
        };
        for (good, per_batch) in &plan.inputs {
            goods_ledger::consume_stock(state, bid, *good, per_batch * plan.batches);
        }
        let out_qty = plan.out_per_batch * plan.batches;
        goods_ledger::produce(state, bid, plan.out_good, out_qty);

        // Tool wear: on any day the business actually produced, each equipped
        // worker adds one worker-day of wear; every TOOL_LIFE_WORKER_DAYS of
        // accumulated wear destroys one tool. Breakage is capped by tools on
        // hand; leftover wear carries over and breaks the next tool bought.
        if plan.equipped > 0 {
            let broken = {
                let Some(b) = state.businesses.get_mut(&bid) else {
                    continue;
                };
                b.tool_wear += plan.equipped;
                let breakable = b.tool_wear / TOOL_LIFE_WORKER_DAYS;
                let broken = breakable.min(b.stock(Good::Tools));
                b.tool_wear -= broken * TOOL_LIFE_WORKER_DAYS;
                broken
            };
            if broken > 0 {
                goods_ledger::consume_stock(state, bid, Good::Tools, broken);
            }
        }
        if let Some(b) = state.businesses.get_mut(&bid) {
            b.produced_today = out_qty;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business::TOOL_BONUS_BP;
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

    #[test]
    fn sellers_produce_toward_their_contract_commitments() {
        let mut w = World::from_config(WorldConfig::default_with_seed(5));
        let farm_id = *w
            .state
            .businesses
            .values()
            .find(|b| b.sells == Good::Wheat)
            .map(|b| &b.id)
            .unwrap();
        let buyer = *w
            .state
            .businesses
            .values()
            .find(|b| b.sells == Good::Flour)
            .map(|b| &b.id)
            .unwrap();
        // At its EMA target the farm would sit idle; a 12-unit weekly
        // commitment reopens the gap and the farm builds toward it.
        {
            let farm = w.state.businesses.get_mut(&farm_id).unwrap();
            farm.sales_ema_milli = 2_000; // expects 2/day → EMA target 10
            farm.inventory.clear();
            farm.add_stock(Good::Wheat, 10);
        }
        run(&mut w.state);
        assert_eq!(
            w.state.businesses[&farm_id].produced_today, 0,
            "at the EMA target without commitments"
        );
        crate::contracts::sign(
            &mut w.state,
            &mut w.journal,
            0,
            crate::contracts::SupplyTerms {
                seller: farm_id,
                buyer,
                good: Good::Wheat,
                qty: 12,
                unit_price: crate::money::Money::from_cents(500),
            },
        );
        run(&mut w.state);
        assert!(
            w.state.businesses[&farm_id].produced_today > 0,
            "the commitment reopens the production gap"
        );
    }

    #[test]
    fn perishable_output_targets_a_one_day_buffer() {
        let mut w = World::from_config(WorldConfig::default_with_seed(5));
        let bakery_id = *w
            .state
            .businesses
            .values()
            .find(|b| b.sells == Good::Food)
            .map(|b| &b.id)
            .unwrap();
        // Expects 10/day: perishable target = 10 × (2 + 1) = 30, far below
        // the durable formula's 50.
        {
            let b = w.state.businesses.get_mut(&bakery_id).unwrap();
            b.sales_ema_milli = 10_000;
            b.inventory.clear();
            b.add_stock(Good::Food, 30);
            b.add_stock(Good::Flour, 100);
        }
        run(&mut w.state);
        assert_eq!(
            w.state.businesses[&bakery_id].produced_today, 0,
            "at the perishable target: no production"
        );
        {
            let b = w.state.businesses.get_mut(&bakery_id).unwrap();
            b.add_stock(Good::Food, -30);
        }
        run(&mut w.state);
        assert_eq!(
            w.state.businesses[&bakery_id].produced_today, 30,
            "refills exactly to the perishable buffer plus today's sales"
        );
    }

    #[test]
    fn equipped_workers_produce_the_tool_bonus() {
        let mut w = World::from_config(WorldConfig::default_with_seed(5));
        let farm_id = *w
            .state
            .businesses
            .values()
            .find(|b| b.sells == Good::Wheat)
            .map(|b| &b.id)
            .unwrap();
        let (workers, bpw) = {
            let farm = w.state.businesses.get_mut(&farm_id).unwrap();
            assert!(farm.uses_tools, "farms are tool users");
            farm.inventory.clear();
            farm.add_stock(Good::Tools, farm.workers.len() as Qty);
            farm.sales_ema_milli = 1_000_000; // capacity-bound
            (farm.workers.len() as Qty, farm.recipe.batches_per_worker)
        };
        assert!(workers > 0);
        run(&mut w.state);
        let farm = &w.state.businesses[&farm_id];
        let base = workers * bpw;
        let bonus = workers * bpw * TOOL_BONUS_BP / 10_000;
        assert!(bonus > 0, "farm bpw must yield a whole bonus batch");
        assert_eq!(farm.produced_today, base + bonus);
        assert_eq!(farm.tool_wear, workers, "one worker-day of wear each");
    }

    #[test]
    fn tools_wear_out_and_are_destroyed_through_the_goods_ledger() {
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
            farm.inventory.clear();
            farm.add_stock(Good::Tools, 1);
            // Register the out-of-band test inventory in the conservation
            // target, then keep demand huge so the farm produces daily.
            farm.sales_ema_milli = 1_000_000;
        }
        w.state.expected_total_goods.clear();
        for good in Good::ALL {
            let total = w.state.total_goods(good);
            w.state.expected_total_goods.insert(good, total);
        }
        // One tool equips one worker: it breaks after TOOL_LIFE_WORKER_DAYS
        // production days.
        for _ in 0..TOOL_LIFE_WORKER_DAYS {
            assert_eq!(w.state.businesses[&farm_id].stock(Good::Tools), 1);
            run(&mut w.state);
            crate::invariants::check_all(&w.state, &w.journal).unwrap();
        }
        let farm = &w.state.businesses[&farm_id];
        assert_eq!(farm.stock(Good::Tools), 0, "tool must wear out");
        assert_eq!(farm.tool_wear, 0, "wear resets when the tool breaks");
    }

    #[test]
    fn no_tools_means_no_bonus_and_no_wear() {
        let mut w = World::from_config(WorldConfig::default_with_seed(5));
        let farm_id = *w
            .state
            .businesses
            .values()
            .find(|b| b.sells == Good::Wheat)
            .map(|b| &b.id)
            .unwrap();
        let (workers, bpw) = {
            let farm = w.state.businesses.get_mut(&farm_id).unwrap();
            farm.inventory.clear();
            farm.sales_ema_milli = 1_000_000;
            (farm.workers.len() as Qty, farm.recipe.batches_per_worker)
        };
        run(&mut w.state);
        let farm = &w.state.businesses[&farm_id];
        assert_eq!(farm.produced_today, workers * bpw);
        assert_eq!(farm.tool_wear, 0);
    }
}
