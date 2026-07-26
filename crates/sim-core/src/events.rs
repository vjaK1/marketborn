//! Simulation events: the observable record of what happened.
//!
//! Events are outputs — they are journaled, archived to SQLite on save, and
//! shown in the UI, but they are *not* part of the hashed state.
//! Determinism tests compare event sequences explicitly.

use crate::contracts::ContractParty;
use crate::goods::{Good, Qty};
use crate::ids::{AccountId, AgentId, BusinessId, ContractId, LoanId};
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
    /// A worker left one job for a better-paying one (Phase 2 labor
    /// mobility).
    JobSwitched {
        agent: AgentId,
        from: BusinessId,
        to: BusinessId,
        old_wage: Money,
        new_wage: Money,
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
    /// A moribund business changed hands: a wealthy agent bought it from
    /// its broke owner to restart it (DECISIONS.md #021).
    BusinessSold {
        business: BusinessId,
        from: AgentId,
        to: AgentId,
        price: Money,
    },
    /// A supply contract was agreed (Phase 3): recurring delivery of
    /// `qty` × `good` at `unit_price`, daily, `deliveries` times.
    ContractSigned {
        contract: ContractId,
        seller: BusinessId,
        buyer: BusinessId,
        good: Good,
        qty: Qty,
        unit_price: Money,
        deliveries: u32,
    },
    /// A scheduled delivery settled: goods moved and the buyer paid.
    ContractDelivered {
        contract: ContractId,
        good: Good,
        qty: Qty,
        amount: Money,
    },
    /// A scheduled delivery failed; the failing side paid a (cash-capped)
    /// penalty.
    ContractMissed {
        contract: ContractId,
        by: ContractParty,
        penalty: Money,
    },
    /// Terminated after too many consecutive misses.
    ContractBreached {
        contract: ContractId,
        by: ContractParty,
    },
    /// A party walked away voluntarily, paying the exit penalty (the
    /// brief's "breach contract" action — an underwater buyer cuts its
    /// losses).
    ContractTerminated {
        contract: ContractId,
        by: ContractParty,
        penalty: Money,
    },
    /// Every scheduled delivery date has passed; the contract ended
    /// normally.
    ContractCompleted {
        contract: ContractId,
    },
    /// The bank issued a working-capital loan (Phase 3).
    LoanIssued {
        loan: LoanId,
        business: BusinessId,
        principal: Money,
        rate_bp: i64,
    },
    /// A day's loan service could not be met in full.
    LoanPaymentMissed {
        loan: LoanId,
        business: BusinessId,
        shortfall: Money,
    },
    /// Principal fully repaid; the loan closed cleanly.
    LoanRepaid {
        loan: LoanId,
        business: BusinessId,
    },
    /// Three consecutive missed payments: the loan defaulted.
    LoanDefaulted {
        loan: LoanId,
        business: BusinessId,
        outstanding: Money,
    },
    /// Foreclosure ran: cash and goods seized, the rest written off.
    CollateralSeized {
        loan: LoanId,
        business: BusinessId,
        cash: Money,
        goods_value: Money,
        written_off: Money,
    },
    /// Interest-rate policy moved the bank's base rate.
    BankRateSet {
        old_bp: i64,
        new_bp: i64,
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
            Event::JobSwitched { .. } => "job_switched",
            Event::MissedPayroll { .. } => "missed_payroll",
            Event::PriceChanged { .. } => "price_changed",
            Event::WageChanged { .. } => "wage_changed",
            Event::DividendPaid { .. } => "dividend_paid",
            Event::OwnerInvested { .. } => "owner_invested",
            Event::BusinessSold { .. } => "business_sold",
            Event::ContractSigned { .. } => "contract_signed",
            Event::ContractDelivered { .. } => "contract_delivered",
            Event::ContractMissed { .. } => "contract_missed",
            Event::ContractBreached { .. } => "contract_breached",
            Event::ContractTerminated { .. } => "contract_terminated",
            Event::ContractCompleted { .. } => "contract_completed",
            Event::LoanIssued { .. } => "loan_issued",
            Event::LoanPaymentMissed { .. } => "loan_payment_missed",
            Event::LoanRepaid { .. } => "loan_repaid",
            Event::LoanDefaulted { .. } => "loan_defaulted",
            Event::CollateralSeized { .. } => "collateral_seized",
            Event::BankRateSet { .. } => "bank_rate_set",
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
