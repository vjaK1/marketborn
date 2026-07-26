//! Phase 3 acceptance (third criterion): `probe_rate_shock` — the
//! interest-rate transmission channel exists. Twin runs from the pinned
//! seed diverge only by a `SetBankRate` command: at a punitive rate,
//! distressed owners who would have borrowed choose to struggle instead
//! (the utility engine's borrow scoring prices credit), so lending
//! contracts.
//!
//! Probe policy (TEST_PLAN.md): probes assert a propagation channel, never
//! a scripted outcome — the thresholds were calibrated once against the
//! pinned seed when the probe landed, then frozen as a regression guard.

use sim_core::{Event, PlayerCommand, World, WorldConfig};

const SEED: u64 = 42;
const SHOCK_TICK: u64 = 100;
const HORIZON: u64 = 500;
/// 150%/year: credit priced out of every rational plan.
const PUNITIVE_RATE_BP: i64 = 15_000;

fn loans_issued_after(w: &World, tick: u64) -> usize {
    w.journal
        .events
        .iter()
        .filter(|e| e.tick > tick && matches!(e.event, Event::LoanIssued { .. }))
        .count()
}

#[test]
fn probe_rate_shock_contracts_lending() {
    let mut control = World::from_config(WorldConfig::default_with_seed(SEED));
    control.run_ticks(HORIZON).unwrap();

    let mut shocked = World::from_config(WorldConfig::default_with_seed(SEED));
    shocked
        .queue_command(
            SHOCK_TICK,
            PlayerCommand::SetBankRate {
                rate_bp: PUNITIVE_RATE_BP,
            },
        )
        .unwrap();
    shocked.run_ticks(HORIZON).unwrap();

    // The channel is live: the control town actually borrows in this
    // window (organic distress credit, no staging).
    let control_loans = loans_issued_after(&control, SHOCK_TICK);
    assert!(
        control_loans > 0,
        "control run must borrow after tick {SHOCK_TICK} for the probe to mean anything"
    );

    // The lever reached the street: punitive money finds fewer takers.
    let shocked_loans = loans_issued_after(&shocked, SHOCK_TICK);
    assert!(
        shocked_loans < control_loans,
        "a 150% rate must contract lending: control {control_loans}, shocked {shocked_loans}"
    );

    // And the rate change itself is on the record.
    assert!(shocked.journal.events.iter().any(
        |e| matches!(e.event, Event::BankRateSet { new_bp, .. } if new_bp == PUNITIVE_RATE_BP)
    ));
}
