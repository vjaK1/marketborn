//! Tick-phase systems. Each runs once per tick in the fixed order defined in
//! `docs/ECONOMIC_RULES.md` and orchestrated by [`crate::tick`].

pub mod consumption;
pub mod decisions;
pub mod labor;
pub mod production;
