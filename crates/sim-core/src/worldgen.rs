//! Deterministic world generation: config + seed → initial world.
//!
//! Phase 1 world: the food chain (two farms → mill → bakery) plus the
//! industry chain (mine → steelworks → tool factory), with a configurable
//! population. Parameters and their calibration rationale live in
//! `docs/ECONOMIC_RULES.md` §World parameters.

use crate::agent::Agent;
use crate::business::{Business, BusinessKind, Recipe};
use crate::events::Event;
use crate::goods::{Good, Qty};
use crate::hashing;
use crate::ids::{AgentId, BusinessId};
use crate::money::Money;
use crate::rng::substream;
use crate::world::{InputLog, Journal, MarketState, SimState, SimStatus, World};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldConfig {
    pub master_seed: u64,
    /// Number of agents. Values below 5 are clamped up so every business
    /// has an owner and at least one worker exists.
    pub population: u32,
    /// Hash (and release-build invariant) cadence in ticks. 0 disables
    /// cadence hashing (tests only).
    pub hash_every: u64,
}

impl WorldConfig {
    pub fn default_with_seed(master_seed: u64) -> WorldConfig {
        WorldConfig {
            master_seed,
            population: 26,
            hash_every: 50,
        }
    }
}

const START_AGENT_CASH: Money = Money::from_cents(30_000);
const START_PANTRY: i64 = 3;
const START_BUSINESS_CASH: Money = Money::from_cents(120_000);

const FIRST_NAMES: [&str; 24] = [
    "Ada", "Bram", "Cora", "Dane", "Edda", "Falk", "Greta", "Hugo", "Iris", "Jorn", "Kaia", "Lars",
    "Mara", "Nils", "Odette", "Piet", "Quinn", "Rosa", "Sten", "Tilda", "Ulric", "Vera", "Wim",
    "Ysolde",
];
const LAST_NAMES: [&str; 24] = [
    "Ferro", "Voss", "Aldern", "Brack", "Coste", "Dray", "Ebner", "Falkner", "Grimm", "Hartz",
    "Isen", "Juhl", "Kroll", "Lorentz", "Marsh", "Norde", "Ostler", "Pryce", "Quist", "Rendel",
    "Severin", "Thorn", "Ulm", "Vance",
];

struct BusinessTemplate {
    name: &'static str,
    kind: BusinessKind,
    recipe: Recipe,
    target_headcount: u32,
    wage: Money,
    price: Money,
    stock: &'static [(Good, Qty)],
    sales_ema_milli: i64,
    uses_tools: bool,
}

fn templates() -> Vec<BusinessTemplate> {
    vec![
        BusinessTemplate {
            name: "Northfield Farm",
            kind: BusinessKind::Farm,
            recipe: Recipe {
                inputs: vec![],
                output: (Good::Wheat, 1),
                batches_per_worker: 2,
            },
            target_headcount: 3,
            wage: Money::from_cents(700),
            price: Money::from_cents(550),
            stock: &[(Good::Wheat, 15)],
            sales_ema_milli: 5_000,
            uses_tools: true,
        },
        BusinessTemplate {
            name: "Riverside Farm",
            kind: BusinessKind::Farm,
            recipe: Recipe {
                inputs: vec![],
                output: (Good::Wheat, 1),
                batches_per_worker: 2,
            },
            target_headcount: 3,
            wage: Money::from_cents(700),
            price: Money::from_cents(550),
            stock: &[(Good::Wheat, 15)],
            sales_ema_milli: 5_000,
            uses_tools: true,
        },
        BusinessTemplate {
            name: "Stonebridge Mill",
            kind: BusinessKind::Mill,
            recipe: Recipe {
                inputs: vec![(Good::Wheat, 1)],
                output: (Good::Flour, 1),
                batches_per_worker: 6,
            },
            target_headcount: 3,
            wage: Money::from_cents(700),
            price: Money::from_cents(760),
            stock: &[(Good::Wheat, 10), (Good::Flour, 12)],
            sales_ema_milli: 12_000,
            uses_tools: false,
        },
        BusinessTemplate {
            name: "Hearth & Crust Bakery",
            kind: BusinessKind::Bakery,
            recipe: Recipe {
                inputs: vec![(Good::Flour, 1)],
                output: (Good::Food, 2),
                batches_per_worker: 4,
            },
            target_headcount: 4,
            wage: Money::from_cents(700),
            price: Money::from_cents(540),
            stock: &[(Good::Flour, 12), (Good::Food, 36)],
            sales_ema_milli: 24_000,
            uses_tools: false,
        },
        BusinessTemplate {
            name: "Ironvein Mine",
            kind: BusinessKind::Mine,
            recipe: Recipe {
                inputs: vec![],
                output: (Good::IronOre, 1),
                batches_per_worker: 1,
            },
            target_headcount: 1,
            wage: Money::from_cents(600),
            price: Money::from_cents(750),
            stock: &[(Good::IronOre, 3)],
            sales_ema_milli: 1_000,
            uses_tools: true,
        },
        BusinessTemplate {
            name: "Forgeheart Steelworks",
            kind: BusinessKind::SteelMill,
            recipe: Recipe {
                inputs: vec![(Good::IronOre, 1)],
                output: (Good::Steel, 1),
                batches_per_worker: 1,
            },
            target_headcount: 1,
            wage: Money::from_cents(600),
            price: Money::from_cents(1_500),
            stock: &[(Good::IronOre, 6), (Good::Steel, 4)],
            sales_ema_milli: 1_000,
            uses_tools: false,
        },
        BusinessTemplate {
            name: "Anvil & Edge Toolworks",
            kind: BusinessKind::ToolFactory,
            recipe: Recipe {
                inputs: vec![(Good::Steel, 1)],
                output: (Good::Tools, 1),
                batches_per_worker: 1,
            },
            target_headcount: 1,
            wage: Money::from_cents(600),
            price: Money::from_cents(2_200),
            stock: &[(Good::Steel, 4), (Good::Tools, 6)],
            sales_ema_milli: 1_000,
            uses_tools: false,
        },
    ]
}

pub fn generate(config: WorldConfig) -> World {
    let mut rng = substream(config.master_seed, "worldgen", 0, 0);
    let templates = templates();
    let population = config.population.max(templates.len() as u32 + 1);

    let mut agents: BTreeMap<AgentId, Agent> = BTreeMap::new();
    for i in 0..population {
        let first = FIRST_NAMES[rng.gen_range(0..FIRST_NAMES.len())];
        let last = LAST_NAMES[rng.gen_range(0..LAST_NAMES.len())];
        let id = AgentId(i);
        agents.insert(
            id,
            Agent {
                id,
                name: format!("{first} {last}"),
                cash: START_AGENT_CASH,
                pantry: START_PANTRY,
                employer: None,
                owns: None,
                hungry_streak: 0,
                days_unemployed: 0,
                total_earned: Money::ZERO,
                total_spent: Money::ZERO,
            },
        );
    }

    let mut businesses: BTreeMap<BusinessId, Business> = BTreeMap::new();
    let mut next_worker = templates.len() as u32; // agents after the owners
    for (i, t) in templates.into_iter().enumerate() {
        let bid = BusinessId(i as u32);
        let owner = AgentId(i as u32);
        if let Some(a) = agents.get_mut(&owner) {
            a.owns = Some(bid);
        }
        let mut inventory = BTreeMap::new();
        for (good, qty) in t.stock {
            inventory.insert(*good, *qty);
        }
        let mut workers = Vec::new();
        while workers.len() < t.target_headcount as usize && next_worker < population {
            let aid = AgentId(next_worker);
            next_worker += 1;
            if let Some(a) = agents.get_mut(&aid) {
                a.employer = Some(bid);
                workers.push(aid);
            }
        }
        let sells = t.recipe.output.0;
        businesses.insert(
            bid,
            Business {
                id: bid,
                name: t.name.to_string(),
                kind: t.kind,
                owner,
                cash: START_BUSINESS_CASH,
                workers,
                target_headcount: t.target_headcount,
                wage: t.wage,
                inventory,
                sells,
                price: t.price,
                recipe: t.recipe,
                uses_tools: t.uses_tools,
                tool_wear: 0,
                sales_ema_milli: t.sales_ema_milli,
                stockout_days: 0,
                vacancy_days: 0,
                missed_payroll_days: 0,
                revenue_window: Money::ZERO,
                costs_window: Money::ZERO,
                last_window_profit: Money::ZERO,
                sold_today: 0,
                produced_today: 0,
            },
        );
    }

    let business_count = businesses.len() as u32;
    let mut state = SimState {
        tick: 0,
        config,
        expected_total_money: Money::ZERO,
        expected_total_goods: BTreeMap::new(),
        agents,
        businesses,
        market: MarketState::default(),
        status: SimStatus::Running,
    };
    state.expected_total_money = state.total_cash();
    for good in Good::ALL {
        let total = state.total_goods(good);
        state.expected_total_goods.insert(good, total);
    }

    let mut world = World {
        state,
        inputs: InputLog::default(),
        journal: Journal::default(),
    };
    world.journal.push_event(
        0,
        Event::WorldCreated {
            population,
            businesses: business_count,
        },
    );
    if let Ok(h) = hashing::state_hash(&world.state) {
        world.journal.manifest.push((0, h));
    }
    world
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let a = generate(WorldConfig::default_with_seed(42));
        let b = generate(WorldConfig::default_with_seed(42));
        assert_eq!(
            hashing::state_hash(&a.state).unwrap(),
            hashing::state_hash(&b.state).unwrap()
        );
        assert_eq!(
            a.state.agents[&AgentId(0)].name,
            b.state.agents[&AgentId(0)].name
        );
    }

    #[test]
    fn default_town_shape() {
        let w = generate(WorldConfig::default_with_seed(42));
        assert_eq!(w.state.agents.len(), 26);
        assert_eq!(w.state.businesses.len(), 7);
        let owners = w.state.agents.values().filter(|a| a.owns.is_some()).count();
        assert_eq!(owners, 7);
        let employed = w
            .state
            .agents
            .values()
            .filter(|a| a.employer.is_some())
            .count();
        assert_eq!(employed, 16, "3 + 3 + 3 + 4 + 1 + 1 + 1 staffing plan");
        let tool_users = w.state.businesses.values().filter(|b| b.uses_tools).count();
        assert_eq!(tool_users, 3, "two farms and the mine");
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        for good in Good::ALL {
            assert_eq!(
                w.state.expected_total_goods[&good],
                w.state.total_goods(good),
                "conservation target seeded for {good}"
            );
        }
        assert_eq!(w.journal.manifest.len(), 1, "hash at tick 0");
    }

    #[test]
    fn tiny_population_is_clamped_but_valid() {
        let w = generate(WorldConfig {
            master_seed: 1,
            population: 0,
            hash_every: 50,
        });
        assert_eq!(w.state.agents.len(), 8);
        // All seven businesses exist and have owners; staffing is partial.
        assert_eq!(w.state.businesses.len(), 7);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }
}
