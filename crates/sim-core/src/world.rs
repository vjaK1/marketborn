//! The world: hashed simulation state, input log, and output journal.

use crate::agent::Agent;
use crate::business::Business;
use crate::commands::{CommandError, PlayerCommand, QueuedCommand};
use crate::events::{Event, EventRecord};
use crate::goods::{Good, Qty};
use crate::ids::{AgentId, BusinessId};
use crate::ledger::Transaction;
use crate::metrics::MetricsDay;
use crate::money::Money;
use crate::worldgen::WorldConfig;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// In-memory ring buffer caps (older entries are only in the SQLite archive
/// after a save; see `docs/ARCHITECTURE.md` §Memory bounds).
pub const MAX_EVENTS_IN_MEMORY: usize = 50_000;
pub const MAX_TX_IN_MEMORY: usize = 10_000;
pub const MAX_METRICS_IN_MEMORY: usize = 4_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimStatus {
    Running,
    /// An invariant failed; the simulation refuses to tick further.
    Halted {
        reason: String,
    },
}

/// Cross-market state carried between ticks.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MarketState {
    /// Volume-weighted average execution price of the most recent day with
    /// trades, per good. Businesses use this to estimate input budgets.
    pub last_prices: BTreeMap<Good, Money>,
}

/// The authoritative, hashed simulation state. Everything that influences
/// future simulation outcomes lives here — and only here.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimState {
    /// Number of completed ticks (days). Tick 0 is the freshly generated world.
    pub tick: u64,
    pub config: WorldConfig,
    /// Conservation target: sum of all account balances must equal this.
    /// Only explicit monetary policy may change it.
    pub expected_total_money: Money,
    /// Conservation targets per good: total on-hand quantity across business
    /// inventories and household pantries must equal these. Only the goods
    /// ledger (production mints, consumption/wear burns) may change them —
    /// trades are zero-sum.
    pub expected_total_goods: BTreeMap<Good, Qty>,
    pub agents: BTreeMap<AgentId, Agent>,
    pub businesses: BTreeMap<BusinessId, Business>,
    pub market: MarketState,
    pub status: SimStatus,
}

impl SimState {
    pub fn total_cash(&self) -> Money {
        let agents: Money = self.agents.values().map(|a| a.cash).sum();
        let businesses: Money = self.businesses.values().map(|b| b.cash).sum();
        agents + businesses
    }

    /// Total on-hand quantity of one good across the whole world: business
    /// inventories plus household holdings (pantries hold food; owned homes
    /// are household property).
    pub fn total_goods(&self, good: Good) -> Qty {
        let in_businesses: Qty = self.businesses.values().map(|b| b.stock(good)).sum();
        let in_households: Qty = match good {
            Good::Food => self.agents.values().map(|a| a.pantry).sum(),
            Good::Home => self.agents.values().filter(|a| a.owns_home).count() as Qty,
            _ => 0,
        };
        in_businesses + in_households
    }
}

/// The recorded inputs of the run: every command ever queued, plus the
/// not-yet-applied queue. Saved verbatim; excluded from the state hash
/// (hashes fingerprint state, inputs are the run's cause — DECISIONS.md #003).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InputLog {
    pub command_log: Vec<QueuedCommand>,
    /// Pending commands sorted by `(tick, seq)`.
    pub pending: Vec<QueuedCommand>,
    pub next_seq: u64,
}

/// Outputs: events, transactions, metrics, hash manifest. Saved, shown in
/// the UI, compared in determinism tests — never read by simulation logic.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Journal {
    pub events: VecDeque<EventRecord>,
    pub next_event_seq: u64,
    pub transactions: VecDeque<Transaction>,
    pub next_tx_seq: u64,
    pub metrics: VecDeque<MetricsDay>,
    /// `(tick, blake3 hex)` pairs on the hash cadence.
    pub manifest: Vec<(u64, String)>,
}

impl Journal {
    pub fn push_event(&mut self, tick: u64, event: Event) {
        let record = EventRecord {
            seq: self.next_event_seq,
            tick,
            event,
        };
        self.next_event_seq += 1;
        self.events.push_back(record);
        if self.events.len() > MAX_EVENTS_IN_MEMORY {
            self.events.pop_front();
        }
    }

    pub fn push_metrics(&mut self, day: MetricsDay) {
        self.metrics.push_back(day);
        if self.metrics.len() > MAX_METRICS_IN_MEMORY {
            self.metrics.pop_front();
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct World {
    pub state: SimState,
    pub inputs: InputLog,
    pub journal: Journal,
}

impl World {
    /// Generate a fresh world from config (see `worldgen`).
    pub fn from_config(config: WorldConfig) -> World {
        crate::worldgen::generate(config)
    }

    /// Rebuild a world from config plus a prior run's command log — the
    /// replay entry point. The caller then ticks forward to the target tick.
    pub fn from_config_and_log(config: WorldConfig, log: Vec<QueuedCommand>) -> World {
        let mut world = World::from_config(config);
        world.inputs.next_seq = log.iter().map(|c| c.seq + 1).max().unwrap_or(0);
        world.inputs.pending = log.clone();
        world.inputs.pending.sort_by_key(|c| (c.tick, c.seq));
        world.inputs.command_log = log;
        world
    }

    /// Queue a command for a strictly future tick. Returns its sequence
    /// number. Rejects ticks at or before the current tick — determinism
    /// forbids retroactive input.
    pub fn queue_command(
        &mut self,
        tick: u64,
        command: PlayerCommand,
    ) -> Result<u64, CommandError> {
        if tick <= self.state.tick {
            return Err(CommandError::TickInPast {
                target: tick,
                current: self.state.tick,
            });
        }
        let seq = self.inputs.next_seq;
        self.inputs.next_seq += 1;
        let queued = QueuedCommand { seq, tick, command };
        self.inputs.command_log.push(queued.clone());
        let pos = self
            .inputs
            .pending
            .partition_point(|c| (c.tick, c.seq) <= (tick, seq));
        self.inputs.pending.insert(pos, queued);
        Ok(seq)
    }

    pub fn is_halted(&self) -> bool {
        matches!(self.state.status, SimStatus::Halted { .. })
    }
}
