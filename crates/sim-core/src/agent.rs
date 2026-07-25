//! People. Agents hold cash and a pantry, work for wages, eat, and (from
//! Phase 2) carry personality traits that weight their decisions per
//! `docs/AGENT_DESIGN.md`. Goals, memory and relationships follow.

use crate::ids::{AgentId, BusinessId};
use crate::money::Money;
use serde::{Deserialize, Serialize};

/// Personality, on integer 0–100 scales (50 = neutral), worldgen-rolled
/// from a per-agent substream. Traits *weight* utility scores — they bias
/// choices under conflicting signals, never fully determine them
/// (AGENT_DESIGN.md). Field order is the roll order and must not change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Traits {
    pub risk_tolerance: u8,
    pub time_preference: u8,
    pub loyalty: u8,
    pub honesty: u8,
    pub ambition: u8,
    pub aggression: u8,
    pub patience: u8,
    pub empathy: u8,
    pub greed: u8,
}

impl Traits {
    pub const NEUTRAL: Traits = Traits {
        risk_tolerance: 50,
        time_preference: 50,
        loyalty: 50,
        honesty: 50,
        ambition: 50,
        aggression: 50,
        patience: 50,
        empathy: 50,
        greed: 50,
    };
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub name: String,
    pub cash: Money,
    /// Units of Food stored at home. Consumption draws from here daily.
    pub pantry: i64,
    pub employer: Option<BusinessId>,
    /// The business this agent owns, if any. Owners do not take wage jobs
    /// in Phase 0.
    pub owns: Option<BusinessId>,
    /// Whether this household owns its home (a durable asset bought once
    /// from the construction company; counted in goods conservation).
    pub owns_home: bool,
    /// Personality (Phase 2): weights utility-scored decisions.
    pub traits: Traits,
    /// Bounded personal memory (Phase 2): formed at event sites, decayed
    /// each memory phase, read by decisions (see `memory.rs`).
    pub memories: Vec<crate::memory::Memory>,
    /// Private dyadic relationships toward known counterparties (Phase 2):
    /// sparse, bounded, drifting back to neutral (see `relationships.rs`).
    pub relations: std::collections::BTreeMap<AgentId, crate::relationships::Relationship>,
    /// Consecutive days the agent failed to eat.
    pub hungry_streak: u32,
    /// Consecutive days without employment (owners excluded from job seeking).
    pub days_unemployed: u32,
    /// Lifetime totals, surfaced in the UI.
    pub total_earned: Money,
    pub total_spent: Money,
}

impl Agent {
    pub fn is_job_seeker(&self) -> bool {
        self.employer.is_none() && self.owns.is_none()
    }

    pub fn role_label(&self) -> &'static str {
        if self.owns.is_some() {
            "owner"
        } else if self.employer.is_some() {
            "worker"
        } else {
            "unemployed"
        }
    }
}
