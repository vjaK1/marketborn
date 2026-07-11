//! The tick orchestrator: one call = one simulated day, in the fixed phase
//! order that is part of the determinism contract (`docs/ECONOMIC_RULES.md`).

use crate::commands::PlayerCommand;
use crate::events::Event;
use crate::hashing;
use crate::invariants::{self, InvariantViolation};
use crate::ledger;
use crate::market;
use crate::metrics::{self, DayAccumulator};
use crate::systems;
use crate::world::{SimStatus, World};

#[derive(Debug, thiserror::Error)]
pub enum TickError {
    #[error("simulation is halted: {0}")]
    Halted(String),
    #[error("{0}")]
    Invariant(Box<InvariantViolation>),
    /// An internal inconsistency (e.g. a ledger error on a pre-validated
    /// transfer). Always a bug; the simulation halts loudly.
    #[error("internal simulation error: {0}")]
    Internal(String),
}

#[derive(Clone, Debug)]
pub struct TickReport {
    pub tick: u64,
    /// Set when this tick fell on the hash cadence.
    pub hash: Option<String>,
}

impl World {
    /// Convenience: canonical hash of the current state.
    pub fn state_hash(&self) -> Result<String, TickError> {
        hashing::state_hash(&self.state).map_err(|e| TickError::Internal(e.to_string()))
    }

    /// Advance the world by one day.
    pub fn tick(&mut self) -> Result<TickReport, TickError> {
        if let SimStatus::Halted { reason } = &self.state.status {
            return Err(TickError::Halted(reason.clone()));
        }
        let t = self.state.tick + 1;

        // Reset per-day scratch.
        for b in self.state.businesses.values_mut() {
            b.sold_today = 0;
            b.produced_today = 0;
        }
        let mut acc = DayAccumulator::default();

        // Phase 1 — apply queued commands.
        apply_commands(self, t);
        // Phase 2 — scheduled events: activates in project Phase 4 (no
        // deterministic event types exist yet).
        // Phase 3 — production.
        systems::production::run(&mut self.state);
        // Phase 4 — labor market (matching, payroll).
        systems::labor::run(&mut self.state, &mut self.journal, t).map_err(internal)?;
        // Phase 5 — goods markets (posted prices, deterministic clearing).
        market::run_goods_markets(&mut self.state, &mut self.journal, t, &mut acc)
            .map_err(internal)?;
        // Phase 6 — contract settlement: activates in project Phase 3.
        // Phase 7 — banking: activates in project Phase 3.
        // Phase 8 — consumption.
        systems::consumption::run(&mut self.state, &mut self.journal, t, &mut acc);
        // Phase 9 — agent decisions (Phase 0: business pricing/staffing/
        // dividends; the utility engine arrives in project Phase 2).
        systems::decisions::run(&mut self.state, &mut self.journal, t).map_err(internal)?;
        // Phase 10 — memory & relationship updates: activates in project
        // Phase 2.
        // Phase 11 — bookkeeping: EMAs, metrics, invariants, hashing.
        self.state.tick = t;
        update_sales_emas(self);
        let day = metrics::capture(&self.state, &acc, t);
        self.journal.push_metrics(day);

        let on_cadence =
            self.state.config.hash_every > 0 && t.is_multiple_of(self.state.config.hash_every);
        if cfg!(debug_assertions) || on_cadence {
            if let Err(v) = invariants::check_all(&self.state, &self.journal) {
                self.state.status = SimStatus::Halted {
                    reason: v.summary(),
                };
                return Err(TickError::Invariant(v));
            }
        }
        let mut hash = None;
        if on_cadence {
            let h = self.state_hash()?;
            self.journal.manifest.push((t, h.clone()));
            hash = Some(h);
        }
        Ok(TickReport { tick: t, hash })
    }

    /// Advance `n` days, stopping at the first error.
    pub fn run_ticks(&mut self, n: u64) -> Result<(), TickError> {
        for _ in 0..n {
            self.tick()?;
        }
        Ok(())
    }
}

fn internal(e: impl std::fmt::Display) -> TickError {
    TickError::Internal(e.to_string())
}

/// Phase 1: apply every pending command scheduled for tick `t` (or earlier —
/// can only happen through a bug, applied late rather than dropped), in
/// `(tick, seq)` order. Failed commands become `CommandRejected` events; they
/// never halt the simulation.
fn apply_commands(world: &mut World, t: u64) {
    loop {
        let due = match world.inputs.pending.first() {
            Some(c) if c.tick <= t => world.inputs.pending.remove(0),
            _ => break,
        };
        match due.command {
            PlayerCommand::AdjustMoneySupply {
                account,
                delta,
                memo,
            } => {
                let result = if delta.is_negative() {
                    ledger::burn(
                        &mut world.state,
                        &mut world.journal,
                        t,
                        account,
                        -delta,
                        memo.clone(),
                    )
                } else {
                    ledger::mint(
                        &mut world.state,
                        &mut world.journal,
                        t,
                        account,
                        delta,
                        memo.clone(),
                    )
                };
                match result {
                    Ok(()) => world.journal.push_event(
                        t,
                        Event::MonetaryPolicy {
                            account,
                            delta,
                            memo,
                        },
                    ),
                    Err(e) => world.journal.push_event(
                        t,
                        Event::CommandRejected {
                            seq: due.seq,
                            reason: e.to_string(),
                        },
                    ),
                }
            }
        }
    }
}

/// Integer EMA over daily units sold, in milli-units with a 1/8 step:
/// `ema += (today*1000 - ema) / 8`, rounding toward zero.
fn update_sales_emas(world: &mut World) {
    for b in world.state.businesses.values_mut() {
        let today_milli = b.sold_today * 1000;
        b.sales_ema_milli += (today_milli - b.sales_ema_milli) / 8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::AccountId;
    use crate::money::Money;
    use crate::worldgen::WorldConfig;

    #[test]
    fn ticking_advances_and_stays_conservative() {
        let mut w = World::from_config(WorldConfig::default_with_seed(1));
        w.run_ticks(30).unwrap();
        assert_eq!(w.state.tick, 30);
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        assert!(!w.journal.metrics.is_empty());
    }

    #[test]
    fn hash_cadence_populates_manifest() {
        let mut w = World::from_config(WorldConfig::default_with_seed(1));
        w.run_ticks(100).unwrap();
        // Initial entry at tick 0 plus cadence entries at 50 and 100.
        let ticks: Vec<u64> = w.journal.manifest.iter().map(|(t, _)| *t).collect();
        assert!(ticks.contains(&0));
        assert!(ticks.contains(&50));
        assert!(ticks.contains(&100));
    }

    #[test]
    fn commands_apply_at_their_tick_and_are_causal() {
        let mut w = World::from_config(WorldConfig::default_with_seed(1));
        let target = AccountId::Agent(*w.state.agents.keys().next().unwrap());
        w.queue_command(
            5,
            PlayerCommand::AdjustMoneySupply {
                account: target,
                delta: Money::from_cents(12_345),
                memo: "stimulus test".into(),
            },
        )
        .unwrap();
        let before = w.state.expected_total_money;
        w.run_ticks(4).unwrap();
        assert_eq!(w.state.expected_total_money, before, "not yet applied");
        w.tick().unwrap();
        assert_eq!(
            w.state.expected_total_money,
            before + Money::from_cents(12_345),
            "applied exactly at tick 5"
        );
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    }

    #[test]
    fn queueing_into_the_past_is_rejected() {
        let mut w = World::from_config(WorldConfig::default_with_seed(1));
        w.run_ticks(3).unwrap();
        let target = AccountId::Agent(*w.state.agents.keys().next().unwrap());
        let err = w
            .queue_command(
                3,
                PlayerCommand::AdjustMoneySupply {
                    account: target,
                    delta: Money::from_cents(1),
                    memo: "late".into(),
                },
            )
            .unwrap_err();
        assert!(err.to_string().contains("already at tick 3"));
    }

    #[test]
    fn overdraw_command_is_rejected_as_event_not_halt() {
        let mut w = World::from_config(WorldConfig::default_with_seed(1));
        let target = AccountId::Agent(*w.state.agents.keys().next().unwrap());
        w.queue_command(
            1,
            PlayerCommand::AdjustMoneySupply {
                account: target,
                delta: Money::from_cents(-999_999_999),
                memo: "confiscate".into(),
            },
        )
        .unwrap();
        w.tick().unwrap();
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(e.event, Event::CommandRejected { .. })));
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    }

    #[test]
    fn halted_world_refuses_to_tick() {
        let mut w = World::from_config(WorldConfig::default_with_seed(1));
        w.state.status = SimStatus::Halted {
            reason: "test halt".into(),
        };
        assert!(matches!(w.tick(), Err(TickError::Halted(_))));
    }

    #[test]
    fn corruption_halts_via_invariant_with_report() {
        // hash_every = 1 puts the invariant sweep on every tick in release
        // builds too, so this test is build-profile-independent.
        let mut w = World::from_config(WorldConfig {
            master_seed: 1,
            population: 20,
            hash_every: 1,
        });
        w.run_ticks(2).unwrap();
        let id = *w.state.agents.keys().next().unwrap();
        w.state.agents.get_mut(&id).unwrap().cash += Money::from_cents(777);
        let err = w.tick().unwrap_err();
        match err {
            TickError::Invariant(v) => {
                assert_eq!(v.invariant, "money_conservation");
                assert!(!v.recent_transactions.is_empty());
            }
            other => panic!("expected invariant error, got {other:?}"),
        }
        assert!(w.is_halted());
    }
}
