//! `soak_10y` — the Phase 4 decade soak (BRIEF): 3,650 ticks with no
//! player commands; invariants hold at every check and the economy stays
//! non-degenerate. Slow suite: runs under `npm run check:full`.
//!
//! Non-degeneracy is asserted against the pinned seed's real steady
//! state, calibrated once (seed 42, current config): food produced daily
//! (20 on the final day), wheat still repricing (3 distinct traded
//! prices in the last 500 ticks) while food rests at its equilibrium, 12
//! staffed→empty transitions and 6 empty→staffed revivals across the
//! decade, and 13 employed / 6 unemployed at the end.
//!
//! "Business exit and entry" maps to this economy's actual churn
//! channels: businesses are never deleted or founded in v1 — death is a
//! staffed roster emptying, entry is a dead roster staffing back up
//! (injection/takeover/rehiring machinery). Recorded in DECISIONS #031.

use sim_core::{Good, World, WorldConfig};

const SEED: u64 = 42;
const DECADE: u64 = 3_650;

#[test]
#[ignore = "slow soak; part of check:full"]
fn soak_10y_stays_green_and_non_degenerate() {
    let mut w = World::from_config(WorldConfig::default_with_seed(SEED));
    // Debug builds sweep every invariant every tick; any violation halts
    // and fails here.
    w.run_ticks(DECADE).unwrap();
    assert!(!w.is_halted());
    sim_core::invariants::check_all(&w.state, &w.journal).unwrap();

    // The metrics ring (cap 4,000) holds the whole decade.
    assert!(w.journal.metrics.len() as u64 >= DECADE);

    // --- Food production is alive at the end. ---
    let last_30: Vec<_> = w.journal.metrics.iter().rev().take(30).collect();
    let food_produced: i64 = last_30.iter().map(|m| m.food_produced).sum();
    assert!(food_produced > 0, "food production must not collapse");
    assert!(
        last_30
            .iter()
            .any(|m| m.avg_price.get(&Good::Food).copied().flatten().is_some()),
        "food must still be trading"
    );

    // --- The price series is not frozen: at least one staple still
    // repriced within the last 500 ticks (calibrated: wheat takes 3
    // distinct traded prices; a fully frozen tape is the degenerate
    // absorbing state this guards against). ---
    let moving = [Good::Wheat, Good::Flour, Good::Food].iter().any(|g| {
        let mut prices: Vec<i64> = w
            .journal
            .metrics
            .iter()
            .rev()
            .take(500)
            .filter_map(|m| m.avg_price.get(g).copied().flatten())
            .map(|p| p.cents())
            .collect();
        prices.sort_unstable();
        prices.dedup();
        prices.len() >= 2
    });
    assert!(
        moving,
        "some staple must still reprice — a frozen tape is degenerate"
    );

    // --- Churn: at least one business death and one revival over the
    // decade (calibrated: 12 and 6). ---
    let mut exits = 0u32;
    let mut entries = 0u32;
    let ids: Vec<_> = w.state.businesses.keys().copied().collect();
    for id in &ids {
        let mut prev: Option<u32> = None;
        for m in &w.journal.metrics {
            if let Some(bd) = m.businesses.get(id) {
                if let Some(p) = prev {
                    if p > 0 && bd.workers == 0 {
                        exits += 1;
                    }
                    if p == 0 && bd.workers > 0 {
                        entries += 1;
                    }
                }
                prev = Some(bd.workers);
            }
        }
    }
    assert!(
        exits >= 1,
        "no business ever died in a decade — suspiciously static"
    );
    assert!(
        entries >= 1,
        "no dead business ever revived — the recycling machinery is dead"
    );

    // --- Unemployment inside a sane band (calibrated: 13 employed / 6
    // unemployed of 19 non-owners at the end). ---
    let last = w.journal.metrics.back().unwrap();
    assert!(
        last.employed >= 8,
        "employment collapsed: {} employed",
        last.employed
    );
    assert!(
        last.unemployed <= 14,
        "unemployment out of band: {}",
        last.unemployed
    );
}
