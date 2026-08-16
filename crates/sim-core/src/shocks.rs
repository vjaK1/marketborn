//! Deterministic scenario shocks (Phase 4): events that modify underlying
//! CONDITIONS — never outcomes.
//!
//! A shock is triggered by the player command `TriggerShock { kind, days }`
//! (applied at a tick boundary like every command), lives in
//! `SimState.shocks` for its fixed duration, and is retired by tick phase 2
//! at the start of the day it expires — so a `days`-long shock modifies
//! exactly `days` production days. One shock of a kind at a time:
//! re-triggering an active kind is rejected (a `CommandRejected` event),
//! never stacked.
//!
//! The BRIEF's contract: a drought reduces agricultural output; the food
//! shortage, inflation and business failures that follow must emerge from
//! the normal systems. The only mechanical touchpoint is
//! [`capacity_bp`] — the production phase and the price review's
//! utilization base both read it, so a drought-throttled farm neither
//! produces past its withered fields nor mistakes them for idle capacity
//! (which would fire the anti-monopolist price cut against the scarcity).
//!
//! `probe_drought` guards the propagation channel end to end.

use crate::business::BusinessKind;
use crate::events::Event;
use crate::world::{Journal, SimState};
use serde::{Deserialize, Serialize};

/// Farm production capacity lost during a drought, in basis points
/// (5,000 = the fields yield half).
pub const DROUGHT_CAPACITY_CUT_BP: i64 = 5_000;
/// Command clamp on a shock's duration (a decade-long drought is a
/// scenario error, not a scenario).
pub const MAX_SHOCK_DAYS: u32 = 3_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShockKind {
    /// The fields yield half: farm capacity × (1 − cut) for the duration.
    Drought,
}

impl ShockKind {
    pub fn label(self) -> &'static str {
        match self {
            ShockKind::Drought => "drought",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveShock {
    pub kind: ShockKind,
    pub start_tick: u64,
    /// The shock is active on ticks `start_tick..until_tick` and retires
    /// in phase 2 of `until_tick`.
    pub until_tick: u64,
}

/// The production-capacity multiplier a business of `kind` faces today, in
/// basis points (10,000 = unaffected). The single mechanical hook shocks
/// have into the economy: production's batch cap and the price review's
/// utilization base both apply it.
pub fn capacity_bp(state: &SimState, kind: BusinessKind) -> i64 {
    let mut bp = 10_000;
    for s in &state.shocks {
        let cut = match (s.kind, kind) {
            (ShockKind::Drought, BusinessKind::Farm) => DROUGHT_CAPACITY_CUT_BP,
            (ShockKind::Drought, _) => 0,
        };
        bp -= cut;
    }
    bp.max(0)
}

/// Trigger `kind` for `days` at tick `t` (the command site, phase 1).
/// Rejects a kind that is already active — shocks never stack.
pub fn trigger(
    state: &mut SimState,
    journal: &mut Journal,
    t: u64,
    kind: ShockKind,
    days: u32,
) -> Result<(), String> {
    if state.shocks.iter().any(|s| s.kind == kind) {
        return Err(format!("a {} is already active", kind.label()));
    }
    let days = days.clamp(1, MAX_SHOCK_DAYS);
    state.shocks.push(ActiveShock {
        kind,
        start_tick: t,
        until_tick: t + u64::from(days),
    });
    journal.push_event(t, Event::ShockBegan { kind, days });
    Ok(())
}

/// Tick phase 2: retire every shock whose time is up, in list order.
pub fn run(state: &mut SimState, journal: &mut Journal, t: u64) {
    let mut ended = Vec::new();
    state.shocks.retain(|s| {
        if s.until_tick <= t {
            ended.push(s.kind);
            false
        } else {
            true
        }
    });
    for kind in ended {
        journal.push_event(t, Event::ShockEnded { kind });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldConfig;
    use crate::World;

    #[test]
    fn drought_halves_farm_capacity_for_exactly_its_duration() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        assert_eq!(capacity_bp(&w.state, BusinessKind::Farm), 10_000);
        trigger(&mut w.state, &mut w.journal, 10, ShockKind::Drought, 28).unwrap();
        assert_eq!(capacity_bp(&w.state, BusinessKind::Farm), 5_000);
        assert_eq!(
            capacity_bp(&w.state, BusinessKind::Mill),
            10_000,
            "only agriculture withers"
        );
        // Active through tick 37; phase 2 of tick 38 retires it.
        run(&mut w.state, &mut w.journal, 37);
        assert_eq!(capacity_bp(&w.state, BusinessKind::Farm), 5_000);
        run(&mut w.state, &mut w.journal, 38);
        assert_eq!(capacity_bp(&w.state, BusinessKind::Farm), 10_000);
        assert!(w.state.shocks.is_empty());
        assert!(w.journal.events.iter().any(|e| matches!(
            e.event,
            Event::ShockBegan {
                kind: ShockKind::Drought,
                days: 28
            }
        )));
        assert!(w.journal.events.iter().any(|e| matches!(
            e.event,
            Event::ShockEnded {
                kind: ShockKind::Drought
            }
        )));
    }

    #[test]
    fn an_active_shock_kind_cannot_be_retriggered() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        trigger(&mut w.state, &mut w.journal, 5, ShockKind::Drought, 10).unwrap();
        let err = trigger(&mut w.state, &mut w.journal, 6, ShockKind::Drought, 10).unwrap_err();
        assert!(err.contains("already active"));
        assert_eq!(w.state.shocks.len(), 1);
        // After retirement a fresh one is fine.
        run(&mut w.state, &mut w.journal, 15);
        trigger(&mut w.state, &mut w.journal, 20, ShockKind::Drought, 10).unwrap();
        assert_eq!(w.state.shocks.len(), 1);
    }

    #[test]
    fn duration_is_clamped() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        trigger(&mut w.state, &mut w.journal, 1, ShockKind::Drought, 999_999).unwrap();
        assert_eq!(
            w.state.shocks[0].until_tick,
            1 + u64::from(MAX_SHOCK_DAYS),
            "a decade-long drought is a scenario error"
        );
    }
}
