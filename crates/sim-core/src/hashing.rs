//! Canonical state hashing: postcard serialization of [`SimState`] → BLAKE3.
//!
//! Postcard is deterministic for a fixed type layout, and every collection in
//! `SimState` is ordered (`BTreeMap`/`Vec`), so equal states produce equal
//! bytes. The hash covers `SimState` only — inputs and journal are excluded
//! (DECISIONS.md #003); determinism tests compare event sequences separately.

use crate::world::SimState;

#[derive(Debug, thiserror::Error)]
pub enum HashError {
    #[error("canonical serialization failed: {0}")]
    Serialize(#[from] postcard::Error),
}

/// BLAKE3 hex digest of the canonical serialization of `state`.
pub fn state_hash(state: &SimState) -> Result<String, HashError> {
    let bytes = postcard::to_allocvec(state)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use crate::worldgen::WorldConfig;
    use crate::World;

    #[test]
    fn equal_states_hash_equal() {
        let a = World::from_config(WorldConfig::default_with_seed(9));
        let b = World::from_config(WorldConfig::default_with_seed(9));
        assert_eq!(state_hash(&a.state).unwrap(), state_hash(&b.state).unwrap());
    }

    #[test]
    fn state_change_changes_hash() {
        let a = World::from_config(WorldConfig::default_with_seed(9));
        let mut b = World::from_config(WorldConfig::default_with_seed(9));
        let id = *b.state.agents.keys().next().unwrap();
        b.state.agents.get_mut(&id).unwrap().cash += Money::from_cents(1);
        assert_ne!(state_hash(&a.state).unwrap(), state_hash(&b.state).unwrap());
    }

    #[test]
    fn journal_and_inputs_do_not_affect_hash() {
        let a = World::from_config(WorldConfig::default_with_seed(9));
        let mut b = World::from_config(WorldConfig::default_with_seed(9));
        b.journal.push_event(
            0,
            crate::events::Event::WorldCreated {
                population: 0,
                businesses: 0,
            },
        );
        b.inputs.next_seq += 5;
        assert_eq!(state_hash(&a.state).unwrap(), state_hash(&b.state).unwrap());
    }
}
