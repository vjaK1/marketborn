//! `probe_drought` — the Phase 4 emergence probe (TEST_PLAN policy: probes
//! assert that a propagation channel EXISTS, never a scripted outcome;
//! thresholds are calibrated once against this pinned seed, then frozen as
//! regression guards).
//!
//! The BRIEF's contract: inject a drought at a fixed tick; wheat output
//! falls materially, wheat and food prices rise past the pinned thresholds
//! within the horizon, at least one food-chain business raises prices (or
//! posts a negative margin), and all invariants stay green. Everything
//! between the capacity cut and those outcomes — stockouts, the
//! demand-pull channel, the price reviews — is the ordinary machinery.

use sim_core::commands::PlayerCommand;
use sim_core::shocks::ShockKind;
use sim_core::{Event, Good, Money, World, WorldConfig};

const SEED: u64 = 42;
// The mature steady state: production sized to demand, buffers thin — a
// drought during the early post-boom glut is simply absorbed (the first
// calibration run proved it: a 50% capacity cut at tick 200 trimmed
// output 17% and moved no price). The probe wants the channel to BIND.
const SHOCK_TICK: u64 = 600;
const DROUGHT_DAYS: u32 = 84; // a lost season, matching the quarter horizon
const HORIZON: u64 = 800;

/// Mean of the daily volume-weighted average price over `[from, to)`,
/// counting only days the good traded.
fn mean_price(w: &World, good: Good, from: u64, to: u64) -> Option<Money> {
    let mut sum = 0i64;
    let mut days = 0i64;
    for m in &w.journal.metrics {
        if m.tick >= from && m.tick < to {
            if let Some(Some(p)) = m.avg_price.get(&good) {
                sum += p.cents();
                days += 1;
            }
        }
    }
    (days > 0).then(|| Money::from_cents(sum / days))
}

/// Peak daily average price over `[from, to)`.
fn peak_price(w: &World, good: Good, from: u64, to: u64) -> Option<Money> {
    w.journal
        .metrics
        .iter()
        .filter(|m| m.tick >= from && m.tick < to)
        .filter_map(|m| m.avg_price.get(&good).copied().flatten())
        .max()
}

/// Total units the wheat farms produced over `[from, to)`.
fn wheat_produced(w: &World, from: u64, to: u64) -> i64 {
    let farms: Vec<_> = w
        .state
        .businesses
        .values()
        .filter(|b| b.sells == Good::Wheat)
        .map(|b| b.id)
        .collect();
    w.journal
        .metrics
        .iter()
        .filter(|m| m.tick >= from && m.tick < to)
        .map(|m| {
            farms
                .iter()
                .filter_map(|id| m.businesses.get(id))
                .map(|bd| bd.produced)
                .sum::<i64>()
        })
        .sum()
}

#[test]
fn probe_drought_shortage_propagates_to_prices_through_normal_systems() {
    // Control: the same pinned world, never shocked.
    let mut control = World::from_config(WorldConfig::default_with_seed(SEED));
    control.run_ticks(HORIZON).unwrap();

    // Shocked: identical up to SHOCK_TICK, then a season-long drought.
    let mut w = World::from_config(WorldConfig::default_with_seed(SEED));
    w.queue_command(
        SHOCK_TICK,
        PlayerCommand::TriggerShock {
            kind: ShockKind::Drought,
            days: DROUGHT_DAYS,
        },
    )
    .unwrap();
    w.run_ticks(HORIZON).unwrap();

    let drought_end = SHOCK_TICK + u64::from(DROUGHT_DAYS);

    // --- The condition changed: wheat output falls materially. ---
    // Calibrated once (seed 42, shock at 600): control 627 vs shocked 352
    // (56%) — the 50% capacity cut binds in the mature steady state.
    let control_out = wheat_produced(&control, SHOCK_TICK, drought_end);
    let shocked_out = wheat_produced(&w, SHOCK_TICK, drought_end);
    assert!(
        shocked_out * 100 <= control_out * 75,
        "wheat output must fall materially: control {control_out}, shocked {shocked_out}"
    );

    // --- The shortage propagates to prices through the market. ---
    // Calibrated once: wheat pre-mean $3.53 → shocked peak $5.97 (control
    // peak $3.44, flat); food pre-mean $6.18 → shocked peak $7.91
    // (control $5.71). Frozen guards sit well inside those moves.
    let pre_wheat = mean_price(&w, Good::Wheat, SHOCK_TICK - 50, SHOCK_TICK).unwrap();
    let peak_wheat = peak_price(&w, Good::Wheat, SHOCK_TICK, drought_end + 30).unwrap();
    let pre_food = mean_price(&w, Good::Food, SHOCK_TICK - 50, SHOCK_TICK).unwrap();
    let peak_food = peak_price(&w, Good::Food, SHOCK_TICK, drought_end + 30).unwrap();
    let control_peak_wheat =
        peak_price(&control, Good::Wheat, SHOCK_TICK, drought_end + 30).unwrap();
    let control_peak_food = peak_price(&control, Good::Food, SHOCK_TICK, drought_end + 30).unwrap();
    assert!(
        peak_wheat.cents() * 100 >= pre_wheat.cents() * 130,
        "wheat must spike ≥30% over its pre-drought mean: pre {pre_wheat}, peak {peak_wheat}"
    );
    assert!(
        peak_wheat.cents() * 100 >= control_peak_wheat.cents() * 130,
        "the spike must be the drought's, not the era's: control peak {control_peak_wheat}, shocked {peak_wheat}"
    );
    assert!(
        peak_food.cents() * 100 >= pre_food.cents() * 115,
        "food must follow ≥15% over its pre-drought mean: pre {pre_food}, peak {peak_food}"
    );
    assert!(
        peak_food.cents() * 100 >= control_peak_food.cents() * 115,
        "food's rise must be the drought's: control peak {control_peak_food}, shocked {peak_food}"
    );

    // --- Somebody in the food chain reacted (the BRIEF's "raises prices"
    // arm; calibrated 34 raises in the window). ---
    let chain: Vec<_> = w
        .state
        .businesses
        .values()
        .filter(|b| matches!(b.sells, Good::Wheat | Good::Flour | Good::Food))
        .map(|b| b.id)
        .collect();
    let raises = w
        .journal
        .events
        .iter()
        .filter(|e| e.tick >= SHOCK_TICK && e.tick < drought_end + 30)
        .filter(|e| {
            matches!(&e.event, Event::PriceChanged { business, old, new, .. }
                if chain.contains(business) && new > old)
        })
        .count();
    assert!(
        raises >= 5,
        "the food chain must visibly reprice during the drought (saw {raises})"
    );

    // --- Lifecycle: the shock began, ran its course, and retired. ---
    assert!(w.journal.events.iter().any(|e| matches!(
        e.event,
        Event::ShockBegan {
            kind: ShockKind::Drought,
            ..
        }
    ) && e.tick == SHOCK_TICK));
    assert!(w.journal.events.iter().any(|e| matches!(
        e.event,
        Event::ShockEnded {
            kind: ShockKind::Drought
        }
    ) && e.tick == drought_end));
    assert!(w.state.shocks.is_empty(), "the drought retired");
    assert!(
        !w.is_halted(),
        "invariants stayed green (checked every tick)"
    );
    sim_core::invariants::check_all(&w.state, &w.journal).unwrap();
}
