//! People. Phase 0 agents hold cash and a pantry, work for wages and eat.
//! Traits, goals, memory and relationships arrive in Phase 2 per
//! `docs/AGENT_DESIGN.md`.

use crate::ids::{AgentId, BusinessId};
use crate::money::Money;
use serde::{Deserialize, Serialize};

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
