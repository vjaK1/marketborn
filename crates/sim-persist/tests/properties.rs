//! Phase 6 persistence properties: for ANY seed, population and horizon,
//! a save round-trips to an identical state hash, and a resumed world
//! stays hash-identical to one that never stopped — determinism's
//! save/load contract, swept instead of pinned.

use proptest::prelude::*;
use sim_core::{World, WorldConfig};

fn roundtrip_and_resume(seed: u64, population: u32, ticks: u64) -> Result<(), TestCaseError> {
    let dir = tempfile::tempdir().map_err(|e| TestCaseError::fail(e.to_string()))?;
    let path = dir.path().join("prop.mbsave");
    let mut original = World::from_config(WorldConfig {
        master_seed: seed,
        population,
        hash_every: 25,
    });
    prop_assert!(original.run_ticks(ticks).is_ok());
    sim_persist::save(&original, &path).map_err(|e| TestCaseError::fail(e.to_string()))?;

    let mut resumed = sim_persist::load(&path).map_err(|e| TestCaseError::fail(e.to_string()))?;
    prop_assert_eq!(
        original.state_hash().unwrap(),
        resumed.state_hash().unwrap(),
        "the save IS the world"
    );

    prop_assert!(original.run_ticks(20).is_ok());
    prop_assert!(resumed.run_ticks(20).is_ok());
    prop_assert_eq!(
        original.state_hash().unwrap(),
        resumed.state_hash().unwrap(),
        "resuming equals never having stopped"
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 8, ..ProptestConfig::default() })]

    #[test]
    fn any_world_roundtrips_and_resumes_identically(
        seed in 0u64..10_000,
        population in 3u32..=40,
        ticks in 20u64..=80,
    ) {
        roundtrip_and_resume(seed, population, ticks)?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    #[test]
    #[ignore = "wide property sweep; part of check:full"]
    fn any_world_roundtrips_and_resumes_identically_wide(
        seed in prop::num::u64::ANY,
        population in 3u32..=80,
        ticks in 50u64..=250,
    ) {
        roundtrip_and_resume(seed, population, ticks)?;
    }
}
