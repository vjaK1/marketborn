//! Player commands: the only inbound channel that mutates the world.
//!
//! Commands are queued for a future tick, appended to the command log, and
//! applied in `(tick, seq)` order at the start of that tick. The command log
//! plus the initial config reproduces any run exactly (replay).

use crate::ids::AccountId;
use crate::money::Money;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerCommand {
    /// Explicit monetary policy: mint (positive delta) or withdraw (negative
    /// delta) money at an account. This is the *only* legal money source or
    /// sink in Phase 0; it adjusts the conservation invariant's expected
    /// total. Withdrawals exceeding the account balance are rejected with a
    /// `CommandRejected` event.
    AdjustMoneySupply {
        account: AccountId,
        delta: Money,
        memo: String,
    },
    /// Interest-rate policy (Phase 3): set the bank's annual base rate in
    /// basis points. Existing loans keep their fixed rate; new lending
    /// reprices — the credit-contraction lever `probe_rate_shock` guards.
    /// Clamped to 0..=50_000 bp at application.
    SetBankRate { rate_bp: i64 },
}

impl PlayerCommand {
    pub fn kind(&self) -> &'static str {
        match self {
            PlayerCommand::AdjustMoneySupply { .. } => "adjust_money_supply",
            PlayerCommand::SetBankRate { .. } => "set_bank_rate",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedCommand {
    /// Global submission sequence number; the tie-breaker within a tick.
    pub seq: u64,
    /// The tick at whose start this command applies.
    pub tick: u64,
    pub command: PlayerCommand,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum CommandError {
    #[error("command targets tick {target} but the simulation is already at tick {current}")]
    TickInPast { target: u64, current: u64 },
}
