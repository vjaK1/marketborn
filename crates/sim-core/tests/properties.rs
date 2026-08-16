//! Phase 6 property suite (TEST_PLAN: proptest world generators, integer
//! math and rounding assignment, thousands of ticks under invariants).
//!
//! Fast tiers run in `npm run check`; the wide sweeps (more cases, longer
//! horizons) are `#[ignore]` and run under `check:full`. Rounding
//! properties compare the integer money math against exact i128
//! arithmetic; the world properties assert the one guarantee everything
//! else rests on: no seed, population, horizon or command-surface abuse
//! ever halts an invariant or panics.

use proptest::prelude::*;
use sim_core::commands::PlayerCommand;
use sim_core::ids::AccountId;
use sim_core::ledger;
use sim_core::money::Money;
use sim_core::shocks::ShockKind;
use sim_core::worldgen::WorldConfig;
use sim_core::{AgentId, World};

// ---------- Integer money math ----------

proptest! {
    /// `mul_bp` is exactly x·bp/10000 in i128, truncated toward zero —
    /// the definition every remainder-assignment argument rests on.
    #[test]
    fn mul_bp_matches_exact_i128_truncation(
        cents in -1_000_000_000_000i64..=1_000_000_000_000i64,
        bp in -200_000i64..=200_000i64,
    ) {
        let exact = (i128::from(cents) * i128::from(bp)) / 10_000;
        prop_assert_eq!(i128::from(Money::from_cents(cents).mul_bp(bp).cents()), exact);
    }

    /// The remainder a truncated `mul_bp` leaves behind is strictly
    /// smaller than one unit of the divisor — nothing material is ever
    /// dropped, so "the remainder stays with X" is always a sub-cent
    /// statement.
    #[test]
    fn mul_bp_remainder_is_sub_unit(
        cents in 0i64..=1_000_000_000_000i64,
        bp in 0i64..=50_000i64,
    ) {
        let product = i128::from(cents) * i128::from(bp);
        let kept = i128::from(Money::from_cents(cents).mul_bp(bp).cents()) * 10_000;
        prop_assert!(product - kept < 10_000);
        prop_assert!(product - kept >= 0);
    }

    /// `affordable_units` is the exact floor: what it says you can buy,
    /// you can pay for — and one more would overdraw.
    #[test]
    fn affordable_units_is_the_exact_floor(
        cash in 0i64..=1_000_000_000i64,
        price in 1i64..=10_000_000i64,
    ) {
        let n = Money::from_cents(cash).affordable_units(Money::from_cents(price));
        prop_assert!(n >= 0);
        prop_assert!(n.checked_mul(price).map(|c| c <= cash).unwrap_or(false));
        prop_assert!((n + 1).checked_mul(price).map(|c| c > cash).unwrap_or(true));
    }
}

// ---------- The ledger under arbitrary operation sequences ----------

#[derive(Clone, Debug)]
enum LedgerOp {
    /// Agent→agent transfer (agents carry no books identity, so the
    /// ledger's own guarantees — conservation, atomicity, non-negativity
    /// — are isolated from call-site bookkeeping duties).
    Transfer {
        from: u32,
        to: u32,
        cents: i64,
    },
    Mint {
        to: u32,
        cents: i64,
    },
    Burn {
        from: u32,
        cents: i64,
    },
}

fn ledger_op() -> impl Strategy<Value = LedgerOp> {
    prop_oneof![
        (0u32..40, 0u32..40, -1_000i64..500_000i64)
            .prop_map(|(from, to, cents)| LedgerOp::Transfer { from, to, cents }),
        (0u32..40, 0i64..200_000i64).prop_map(|(to, cents)| LedgerOp::Mint { to, cents }),
        (0u32..40, 0i64..200_000i64).prop_map(|(from, cents)| LedgerOp::Burn { from, cents }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    /// Any sequence of transfers, mints and burns — including overdrafts,
    /// self-transfers, negatives and unknown accounts, all of which must
    /// fail cleanly — leaves total money exactly equal to the adjusted
    /// expectation and every balance non-negative.
    #[test]
    fn the_ledger_never_leaks_under_arbitrary_ops(
        seed in 0u64..1_000,
        ops in prop::collection::vec(ledger_op(), 1..60),
    ) {
        let mut w = World::from_config(WorldConfig::default_with_seed(seed));
        for op in ops {
            match op {
                LedgerOp::Transfer { from, to, cents } => {
                    let _ = ledger::transfer(
                        &mut w.state,
                        &mut w.journal,
                        1,
                        AccountId::Agent(AgentId(from)),
                        AccountId::Agent(AgentId(to)),
                        Money::from_cents(cents),
                        sim_core::TxKind::Wage,
                    );
                }
                LedgerOp::Mint { to, cents } => {
                    let _ = ledger::mint(
                        &mut w.state,
                        &mut w.journal,
                        1,
                        AccountId::Agent(AgentId(to)),
                        Money::from_cents(cents),
                        "fuzz".into(),
                    );
                }
                LedgerOp::Burn { from, cents } => {
                    let _ = ledger::burn(
                        &mut w.state,
                        &mut w.journal,
                        1,
                        AccountId::Agent(AgentId(from)),
                        Money::from_cents(cents),
                        "fuzz".into(),
                    );
                }
            }
        }
        prop_assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        prop_assert!(sim_core::invariants::check_all(&w.state, &w.journal).is_ok());
    }
}

// ---------- Whole worlds under invariants ----------

fn assert_world_stays_green(seed: u64, population: u32, ticks: u64) -> Result<(), TestCaseError> {
    let mut w = World::from_config(WorldConfig {
        master_seed: seed,
        population,
        hash_every: 50,
    });
    // Debug builds sweep all nine invariants every tick; any violation
    // or panic fails the case and proptest shrinks it.
    prop_assert!(w.run_ticks(ticks).is_ok(), "halted: {:?}", w.state.status);
    prop_assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 10, ..ProptestConfig::default() })]

    /// Any seed × population × horizon produces a world that ticks green.
    #[test]
    fn arbitrary_worlds_stay_green(
        seed in prop::num::u64::ANY,
        population in 3u32..=50,
        ticks in 30u64..=120,
    ) {
        assert_world_stays_green(seed, population, ticks)?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, ..ProptestConfig::default() })]

    /// The wide sweep: bigger towns, longer horizons (check:full tier).
    #[test]
    #[ignore = "wide property sweep; part of check:full"]
    fn arbitrary_worlds_stay_green_wide(
        seed in prop::num::u64::ANY,
        population in 3u32..=100,
        ticks in 100u64..=400,
    ) {
        assert_world_stays_green(seed, population, ticks)?;
    }
}

// ---------- The command surface under abuse ----------

fn arbitrary_command() -> impl Strategy<Value = PlayerCommand> {
    prop_oneof![
        (0u32..60, -2_000_000i64..2_000_000i64).prop_map(|(id, delta)| {
            PlayerCommand::AdjustMoneySupply {
                account: AccountId::Agent(AgentId(id)),
                delta: Money::from_cents(delta),
                memo: "fuzz".into(),
            }
        }),
        (-10_000i64..200_000i64).prop_map(|rate_bp| PlayerCommand::SetBankRate { rate_bp }),
        (-10_000i64..100_000i64).prop_map(|rate_bp| PlayerCommand::SetSalesTax { rate_bp }),
        (-100_000i64..10_000_000i64).prop_map(|floor| PlayerCommand::SetWelfareFloor {
            floor: Money::from_cents(floor)
        }),
        (-100_000i64..10_000_000i64).prop_map(|wage| PlayerCommand::SetMinimumWage {
            wage: Money::from_cents(wage)
        }),
        (-100_000i64..1_000_000_000i64).prop_map(|limit| {
            PlayerCommand::SetDeficitLimit {
                limit: Money::from_cents(limit),
            }
        }),
        (0u32..20_000).prop_map(|days| PlayerCommand::TriggerShock {
            kind: ShockKind::Drought,
            days,
        }),
    ]
}

fn assert_commands_cannot_corrupt(
    seed: u64,
    ticks: u64,
    commands: Vec<(u64, PlayerCommand)>,
) -> Result<(), TestCaseError> {
    let mut w = World::from_config(WorldConfig::default_with_seed(seed));
    for (at, cmd) in commands {
        // Queue across the horizon; ticks in the past are rejected by the
        // queue itself (none here — at ≥ 1).
        let _ = w.queue_command(1 + (at % ticks), cmd);
    }
    prop_assert!(
        w.run_ticks(ticks + 20).is_ok(),
        "the command surface halted the world: {:?}",
        w.state.status
    );
    prop_assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    prop_assert!(sim_core::invariants::check_all(&w.state, &w.journal).is_ok());
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 10, ..ProptestConfig::default() })]

    /// The player can pull any lever with any value at any time — clamps
    /// and rejections absorb it all; the world never halts or leaks.
    #[test]
    fn the_command_surface_cannot_corrupt_the_world(
        seed in 0u64..1_000,
        ticks in 40u64..=120,
        commands in prop::collection::vec((0u64..200, arbitrary_command()), 0..25),
    ) {
        assert_commands_cannot_corrupt(seed, ticks, commands)?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, ..ProptestConfig::default() })]

    /// The wide command-abuse sweep (check:full tier).
    #[test]
    #[ignore = "wide property sweep; part of check:full"]
    fn the_command_surface_cannot_corrupt_the_world_wide(
        seed in prop::num::u64::ANY,
        ticks in 100u64..=300,
        commands in prop::collection::vec((0u64..400, arbitrary_command()), 0..60),
    ) {
        assert_commands_cannot_corrupt(seed, ticks, commands)?;
    }
}
