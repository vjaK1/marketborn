//! The delayed-policy-effect test (Phase 4, BRIEF: "policies have costs,
//! tradeoffs and delayed effects").
//!
//! Scenario: at tick 600 the sales tax is abolished — and with it the
//! welfare floor it funds (the treasury is pinned at zero in steady
//! state; no intake, no dole). Nothing breaks that day: the last
//! recipients still hold their $12 floats, pantries carry a few meals,
//! and prices take weeks to re-equilibrate around the missing demand.
//! The cost arrives with a LAG: mean hunger climbs from the welfare
//! equilibrium (~14 hungry) back toward the no-welfare equilibrium
//! (~20 — the E0/E4 calibration gap of ADR #029), months later.
//!
//! Two hikes were tried first and REJECTED as scenarios: in the mature
//! steady state both seed 42 and marginal seed 123 absorb even a 9%
//! sales tax indefinitely — the dole recycles the entire take into
//! final demand, so taxation there redistributes instead of
//! contracting (business cash ends HIGHER under the hike). The
//! delayed effect this economy actually exhibits is the one measured
//! here: the welfare state's abolition starves quietly, a season later.

use sim_core::commands::PlayerCommand;
use sim_core::{World, WorldConfig};

const SEED: u64 = 42;
const POLICY_TICK: u64 = 600;
const HORIZON: u64 = 1_500;

fn mean_hungry(w: &World, from: u64, to: u64) -> f64 {
    let (mut sum, mut n) = (0u64, 0u64);
    for m in &w.journal.metrics {
        if m.tick >= from && m.tick < to {
            sum += u64::from(m.hungry);
            n += 1;
        }
    }
    sum as f64 / n.max(1) as f64
}

fn recipients(w: &World, from: u64, to: u64) -> u64 {
    w.journal
        .metrics
        .iter()
        .filter(|m| m.tick >= from && m.tick < to)
        .map(|m| u64::from(m.welfare_recipients))
        .sum()
}

#[test]
fn abolishing_the_welfare_state_starves_with_a_lag() {
    let mut control = World::from_config(WorldConfig::default_with_seed(SEED));
    control.run_ticks(HORIZON).unwrap();

    let mut w = World::from_config(WorldConfig::default_with_seed(SEED));
    w.queue_command(POLICY_TICK, PlayerCommand::SetSalesTax { rate_bp: 0 })
        .unwrap();
    w.run_ticks(HORIZON).unwrap();

    // --- The cause is immediate: collections freeze, the dole dries up
    // within days of the treasury draining. ---
    assert_eq!(
        w.state.government.books.tax_collected,
        {
            let mut at_cut = sim_core::Money::ZERO;
            for m in &w.journal.metrics {
                if m.tick == POLICY_TICK {
                    at_cut = m.tax_collected;
                }
            }
            at_cut
        },
        "a zero rate collects nothing further"
    );
    let dole_before = recipients(&w, POLICY_TICK - 100, POLICY_TICK);
    let dole_after = recipients(&w, POLICY_TICK + 30, POLICY_TICK + 130);
    assert!(
        dole_before >= 50,
        "the welfare state must have been real before abolition ({dole_before} payments)"
    );
    assert_eq!(dole_after, 0, "no funding, no dole");

    // --- The effect is NOT immediate. Calibrated once (seed 42): the
    // first fortnight reads 17.29 abolished vs 17.14 control (+0.15),
    // and the first three months stay within +0.10 — the last floats,
    // pantries and standing prices carry the poor for a while. ---
    let early_c = mean_hungry(&control, POLICY_TICK, POLICY_TICK + 14);
    let early_w = mean_hungry(&w, POLICY_TICK, POLICY_TICK + 14);
    assert!(
        (early_w - early_c).abs() <= 1.0,
        "abolition must not bite in the first fortnight: control {early_c:.2}, abolished {early_w:.2}"
    );
    let q_c = mean_hungry(&control, POLICY_TICK + 14, POLICY_TICK + 100);
    let q_w = mean_hungry(&w, POLICY_TICK + 14, POLICY_TICK + 100);
    assert!(
        q_w - q_c <= 1.0,
        "the first quarter still reads near the welfare equilibrium: control {q_c:.2}, abolished {q_w:.2}"
    );

    // --- ...and it HAS arrived a year later. Calibrated once: control
    // 14.40/14.27 vs abolished 20.23/19.95 over [1100,1500) — the gap
    // (+5.7) is the E0-vs-E4 equilibrium difference of ADR #029
    // reappearing on schedule. Frozen guard: ≥ 3 more hungry. ---
    let late_c = mean_hungry(&control, 1_100, HORIZON);
    let late_w = mean_hungry(&w, 1_100, HORIZON);
    assert!(
        late_w - late_c >= 3.0,
        "the cost must have arrived by year 3: control {late_c:.2}, abolished {late_w:.2}"
    );

    assert!(!w.is_halted());
    sim_core::invariants::check_all(&w.state, &w.journal).unwrap();
}
