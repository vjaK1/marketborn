//! Tradable goods. Phase 0 carried the minimal food chain; Phase 1 adds the
//! industry chain (iron ore → steel → tools). Construction goods (wood,
//! bricks, buildings) arrive later in Phase 1.

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
    IronOre,
    Steel,
    /// Capital good: extraction businesses equip workers with tools for a
    /// production bonus; tools wear out with use (see `business.rs`).
    Tools,
    Wood,
    Bricks,
    /// Durable asset: built by the construction company, bought once by a
    /// wealthy household. Owned homes leave business inventory but stay in
    /// the conservation totals (DECISIONS.md #017).
    Home,
}

impl Good {
    /// All goods in canonical market order. Goods markets clear in this order
    /// every tick (part of the determinism contract). New goods append; the
    /// existing order never reshuffles.
    pub const ALL: [Good; 9] = [
        Good::Wheat,
        Good::Flour,
        Good::Food,
        Good::IronOre,
        Good::Steel,
        Good::Tools,
        Good::Wood,
        Good::Bricks,
        Good::Home,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Good::Wheat => "wheat",
            Good::Flour => "flour",
            Good::Food => "food",
            Good::IronOre => "iron ore",
            Good::Steel => "steel",
            Good::Tools => "tools",
            Good::Wood => "wood",
            Good::Bricks => "bricks",
            Good::Home => "home",
        }
    }

    /// Daily spoilage rate in basis points of each holder's stock, rounded
    /// toward zero per holder per day (the sub-unit remainder stays fresh —
    /// small stocks like pantries never rot). 0 = durable. Grains keep;
    /// prepared food does not (ECONOMIC_RULES.md §Consumption).
    pub fn spoilage_bp(self) -> i64 {
        match self {
            Good::Food => 400,
            Good::Wheat
            | Good::Flour
            | Good::IronOre
            | Good::Steel
            | Good::Tools
            | Good::Wood
            | Good::Bricks
            | Good::Home => 0,
        }
    }

    pub fn is_perishable(self) -> bool {
        self.spoilage_bp() > 0
    }
}

impl fmt::Display for Good {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
