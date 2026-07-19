//! Marketborn simulation core.
//!
//! Pure, deterministic simulation logic. This crate performs no I/O, reads no
//! wall clock, spawns no threads, and uses no RNG other than seeded
//! [`rand_chacha::ChaCha8Rng`] substreams derived in [`rng`].
//!
//! Determinism contract (same build, same platform): identical seed, initial
//! config and command log produce identical state hashes and identical event
//! sequences. See `docs/ECONOMIC_RULES.md` for the binding tick phase order.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod agent;
pub mod business;
pub mod commands;
pub mod events;
pub mod goods;
pub mod goods_ledger;
pub mod hashing;
pub mod ids;
pub mod invariants;
pub mod ledger;
pub mod market;
pub mod metrics;
pub mod money;
pub mod rng;
pub mod snapshot;
pub mod systems;
pub mod tick;
pub mod world;
pub mod worldgen;

pub use agent::Agent;
pub use business::{Business, BusinessKind, Recipe};
pub use commands::{PlayerCommand, QueuedCommand};
pub use events::{Event, EventRecord};
pub use goods::{Good, Qty};
pub use ids::{AccountId, AgentId, BusinessId};
pub use invariants::InvariantViolation;
pub use ledger::{Transaction, TxKind};
pub use money::Money;
pub use snapshot::WorldSnapshot;
pub use tick::{TickError, TickReport};
pub use world::{SimState, World};
pub use worldgen::WorldConfig;
