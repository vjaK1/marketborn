//! Deterministic RNG substreams.
//!
//! A master seed derives named substreams via BLAKE3 over
//! `(master_seed, stream name, entity id, tick)`. Streams are derived fresh
//! at each use site — no mutable RNG state is stored in the world, so the
//! master seed alone (plus tick/entity context) fully determines every draw.
//! Adding a new consumer never reshuffles randomness for existing ones.
//! (DECISIONS.md #002.)

use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Derive the substream for `(stream, entity, tick)` under `master_seed`.
///
/// Use `entity = 0` for world-level streams and `tick = 0` for one-shot
/// streams such as world generation.
pub fn substream(master_seed: u64, stream: &str, entity: u64, tick: u64) -> ChaCha8Rng {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"marketborn.rng.v1");
    hasher.update(&master_seed.to_le_bytes());
    hasher.update(&(stream.len() as u64).to_le_bytes());
    hasher.update(stream.as_bytes());
    hasher.update(&entity.to_le_bytes());
    hasher.update(&tick.to_le_bytes());
    ChaCha8Rng::from_seed(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn same_inputs_same_stream() {
        let mut a = substream(42, "worldgen", 0, 0);
        let mut b = substream(42, "worldgen", 0, 0);
        for _ in 0..16 {
            assert_eq!(a.gen::<u64>(), b.gen::<u64>());
        }
    }

    #[test]
    fn different_name_entity_or_tick_diverges() {
        let base: u64 = substream(42, "worldgen", 0, 0).gen();
        assert_ne!(base, substream(42, "labor", 0, 0).gen::<u64>());
        assert_ne!(base, substream(42, "worldgen", 1, 0).gen::<u64>());
        assert_ne!(base, substream(42, "worldgen", 0, 1).gen::<u64>());
        assert_ne!(base, substream(43, "worldgen", 0, 0).gen::<u64>());
    }

    #[test]
    fn name_length_is_domain_separated() {
        // "ab" + entity bytes must not collide with "abc" + shifted bytes:
        // the length prefix prevents concatenation ambiguity.
        let a: u64 = substream(1, "ab", 0x63, 0).gen();
        let b: u64 = substream(1, "abc", 0, 0).gen();
        assert_ne!(a, b);
    }
}
