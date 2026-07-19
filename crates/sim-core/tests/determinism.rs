//! Determinism contract tests: identical seed + identical command log ⇒
//! identical state hashes and identical event sequences.

use sim_core::{AccountId, AgentId, BusinessId, Money, PlayerCommand, World, WorldConfig};

fn test_commands() -> Vec<(u64, PlayerCommand)> {
    vec![
        (
            30,
            PlayerCommand::AdjustMoneySupply {
                account: AccountId::Agent(AgentId(12)),
                delta: Money::from_cents(50_000),
                memo: "northern aid package".into(),
            },
        ),
        (
            200,
            PlayerCommand::AdjustMoneySupply {
                account: AccountId::Business(BusinessId(2)),
                delta: Money::from_cents(-25_000),
                memo: "windfall levy".into(),
            },
        ),
    ]
}

fn run_world(seed: u64, ticks: u64, with_commands: bool) -> World {
    let mut w = World::from_config(WorldConfig::default_with_seed(seed));
    if with_commands {
        for (tick, cmd) in test_commands() {
            w.queue_command(tick, cmd).expect("queue in the future");
        }
    }
    w.run_ticks(ticks).expect("run stays green");
    w
}

fn event_stream(w: &World) -> Vec<String> {
    w.journal
        .events
        .iter()
        .map(|r| format!("{}:{}:{:?}", r.seq, r.tick, r.event))
        .collect()
}

#[test]
fn twin_runs_produce_identical_hashes_and_events() {
    let a = run_world(42, 400, true);
    let b = run_world(42, 400, true);
    assert!(
        a.journal.manifest.len() >= 9,
        "manifest should have tick 0 + cadence entries, got {}",
        a.journal.manifest.len()
    );
    assert_eq!(a.journal.manifest, b.journal.manifest, "hash manifests");
    assert_eq!(
        a.state_hash().unwrap(),
        b.state_hash().unwrap(),
        "final state hash"
    );
    assert_eq!(event_stream(&a), event_stream(&b), "event sequences");
    assert_eq!(a.journal.metrics, b.journal.metrics, "metrics series");
    assert!(
        !a.journal.decisions.is_empty(),
        "price reviews must journal decision records"
    );
    let decisions = |w: &sim_core::World| -> Vec<(u64, u32, sim_core::PriceAction)> {
        w.journal
            .decisions
            .iter()
            .map(|d| (d.tick, d.business.0, d.chosen))
            .collect()
    };
    assert_eq!(decisions(&a), decisions(&b), "decision sequences");
}

#[test]
fn different_seeds_diverge() {
    let a = run_world(1, 60, false);
    let b = run_world(2, 60, false);
    assert_ne!(
        a.state_hash().unwrap(),
        b.state_hash().unwrap(),
        "different seeds must produce different worlds"
    );
}

#[test]
fn commands_are_causal() {
    let with = run_world(42, 60, true);
    let without = run_world(42, 60, false);
    assert_ne!(
        with.state_hash().unwrap(),
        without.state_hash().unwrap(),
        "a tick-30 monetary policy command must alter the world"
    );
}

#[test]
fn replay_from_command_log_reproduces_the_run() {
    let original = run_world(42, 400, true);
    let mut replayed = World::from_config_and_log(
        WorldConfig::default_with_seed(42),
        original.inputs.command_log.clone(),
    );
    replayed.run_ticks(400).expect("replay stays green");
    assert_eq!(
        original.journal.manifest, replayed.journal.manifest,
        "replay manifest must match the original run"
    );
    assert_eq!(
        original.state_hash().unwrap(),
        replayed.state_hash().unwrap()
    );
    assert_eq!(event_stream(&original), event_stream(&replayed));
}

/// Long soak: the economy must stay non-degenerate with no player input.
/// Slow suite: runs under `npm run check:full`.
#[test]
#[ignore = "slow soak; part of check:full"]
fn soak_1500_ticks_stays_alive_and_green() {
    let w = run_world(42, 1500, false);
    assert!(!w.is_halted());
    let recent: Vec<_> = w.journal.metrics.iter().rev().take(30).collect();
    let food_produced: i64 = recent.iter().map(|m| m.food_produced).sum();
    assert!(food_produced > 0, "food production must not collapse");
    let employed = recent.first().map(|m| m.employed).unwrap_or(0);
    assert!(employed > 0, "someone must still hold a job");
    let any_food_price = recent.iter().any(|m| {
        m.avg_price
            .get(&sim_core::Good::Food)
            .copied()
            .flatten()
            .is_some()
    });
    assert!(any_food_price, "food must still be trading");
}
