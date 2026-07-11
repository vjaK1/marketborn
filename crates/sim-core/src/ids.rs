//! Stable integer entity identifiers.
//!
//! IDs are assigned once at creation and never reused. All simulation-visible
//! iteration is over ordered structures keyed by these IDs.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident, $prefix:literal) => {
        $(#[$doc])*
        #[derive(
            Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize,
        )]
        pub struct $name(pub u32);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", $prefix, self.0)
            }
        }
    };
}

id_type!(
    /// Identifies a person.
    AgentId,
    "A"
);
id_type!(
    /// Identifies a business.
    BusinessId,
    "B"
);

/// A money-holding account. Every ledger movement is between two accounts
/// (or one account and the monetary authority, for explicit mint/burn).
///
/// Ordering note: the derived order (`Business` before `Agent`) is part of
/// the deterministic buyer ordering in the market phase — businesses buy
/// production inputs before households buy consumption goods within the same
/// urgency tier. Documented in `docs/ECONOMIC_RULES.md`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum AccountId {
    Business(BusinessId),
    Agent(AgentId),
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccountId::Business(id) => write!(f, "business {id}"),
            AccountId::Agent(id) => write!(f, "agent {id}"),
        }
    }
}

impl From<AgentId> for AccountId {
    fn from(id: AgentId) -> Self {
        AccountId::Agent(id)
    }
}

impl From<BusinessId> for AccountId {
    fn from(id: BusinessId) -> Self {
        AccountId::Business(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_ordering_puts_businesses_before_agents() {
        let a = AccountId::Agent(AgentId(0));
        let b = AccountId::Business(BusinessId(999));
        assert!(b < a, "businesses must sort before agents");
    }

    #[test]
    fn display_forms() {
        assert_eq!(AgentId(7).to_string(), "A7");
        assert_eq!(BusinessId(2).to_string(), "B2");
        assert_eq!(AccountId::Agent(AgentId(7)).to_string(), "agent A7");
    }
}
