//! Simulation events: the observable record of what happened.
//!
//! Events are outputs — they are journaled, archived to SQLite on save, and
//! shown in the UI, but they are *not* part of the hashed state.
//! Determinism tests compare event sequences explicitly.

use crate::goods::Good;
use crate::ids::{AccountId, AgentId, BusinessId};
use crate::money::Money;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    WorldCreated {
        population: u32,
        businesses: u32,
    },
    Hired {
        agent: AgentId,
        business: BusinessId,
        wage: Money,
    },
    Fired {
        agent: AgentId,
        business: BusinessId,
    },
    /// A worker quit because payroll was missed.
    QuitUnpaid {
        agent: AgentId,
        business: BusinessId,
        owed: Money,
    },
    MissedPayroll {
        business: BusinessId,
        workers_unpaid: u32,
        shortfall: Money,
    },
    PriceChanged {
        business: BusinessId,
        good: Good,
        old: Money,
        new: Money,
    },
    WageChanged {
        business: BusinessId,
        old: Money,
        new: Money,
    },
    DividendPaid {
        business: BusinessId,
        owner: AgentId,
        amount: Money,
    },
    /// The owner put personal savings back into the business.
    OwnerInvested {
        business: BusinessId,
        owner: AgentId,
        amount: Money,
    },
    AgentHungry {
        agent: AgentId,
        streak: u32,
    },
    MonetaryPolicy {
        account: AccountId,
        delta: Money,
        memo: String,
    },
    CommandRejected {
        seq: u64,
        reason: String,
    },
}

impl Event {
    pub fn kind(&self) -> &'static str {
        match self {
            Event::WorldCreated { .. } => "world_created",
            Event::Hired { .. } => "hired",
            Event::Fired { .. } => "fired",
            Event::QuitUnpaid { .. } => "quit_unpaid",
            Event::MissedPayroll { .. } => "missed_payroll",
            Event::PriceChanged { .. } => "price_changed",
            Event::WageChanged { .. } => "wage_changed",
            Event::DividendPaid { .. } => "dividend_paid",
            Event::OwnerInvested { .. } => "owner_invested",
            Event::AgentHungry { .. } => "agent_hungry",
            Event::MonetaryPolicy { .. } => "monetary_policy",
            Event::CommandRejected { .. } => "command_rejected",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord {
    pub seq: u64,
    pub tick: u64,
    pub event: Event,
}
