//! Tradable goods. Phase 0 carries the minimal food chain; the enum grows in
//! Phase 1 (ore, steel, tools, wood, bricks, ...).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Whole units of a good. Quantities are integers and may never go negative
/// in any inventory (invariant-checked).
pub type Qty = i64;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Good {
    Wheat,
    Flour,
    Food,
}

impl Good {
    /// All goods in canonical market order. Goods markets clear in this order
    /// every tick (part of the determinism contract).
    pub const ALL: [Good; 3] = [Good::Wheat, Good::Flour, Good::Food];

    pub fn name(self) -> &'static str {
        match self {
            Good::Wheat => "wheat",
            Good::Flour => "flour",
            Good::Food => "food",
        }
    }
}

impl fmt::Display for Good {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
