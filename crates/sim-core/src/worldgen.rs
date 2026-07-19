//! Deterministic world generation: config + seed → initial world.
//!
//! Phase 1 world: the food chain (two farms → mill → bakery) plus the
//! industry chain (mine → steelworks → tool factory), with a configurable
//! population. Parameters and their calibration rationale live in
//! `docs/ECONOMIC_RULES.md` §World parameters.

use crate::agent::{Agent, Traits};
use crate::business::{Books, Business, BusinessKind, Recipe};
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
            population: 29,
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
    /// Name pool for instances of this kind. Instance `i` takes
    /// `names[i % len]`, with a ` No.k` suffix once the pool wraps.
    names: &'static [&'static str],
    kind: BusinessKind,
    recipe: Recipe,
    target_headcount: u32,
    wage: Money,
    price: Money,
    stock: &'static [(Good, Qty)],
    sales_ema_milli: i64,
    uses_tools: bool,
    /// One instance per this many agents (ceiling division, minimum one).
    /// Calibrated so the default 29-person town gets exactly the audited
    /// business set and 100 agents get a ~20-business economy
    /// (ECONOMIC_RULES.md §World parameters, DECISIONS.md #018).
    per_population: u32,
}

impl BusinessTemplate {
    fn instances(&self, population: u32) -> u32 {
        population.div_ceil(self.per_population).max(1)
    }

    fn instance_name(&self, i: u32) -> String {
        let base = self.names[i as usize % self.names.len()];
        let round = i as usize / self.names.len();
        if round == 0 {
            base.to_string()
        } else {
            format!("{base} No.{}", round + 1)
        }
    }
}

fn templates() -> Vec<BusinessTemplate> {
    vec![
        BusinessTemplate {
            names: &[
                "Northfield Farm",
                "Riverside Farm",
                "Southacre Farm",
                "Millbrook Farm",
                "Hollowdale Farm",
                "Greenridge Farm",
                "Stonefall Farm",
                "Eastmoor Farm",
            ],
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
            per_population: 15,
        },
        BusinessTemplate {
            names: &[
                "Stonebridge Mill",
                "Old Wheel Mill",
                "Graincrest Mill",
                "Twinstone Mill",
            ],
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
            per_population: 35,
        },
        BusinessTemplate {
            names: &[
                "Hearth & Crust Bakery",
                "Morning Loaf Bakery",
                "Golden Oven Bakery",
                "Cinder & Crumb Bakery",
            ],
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
            per_population: 30,
        },
        BusinessTemplate {
            names: &["Ironvein Mine", "Deepshaft Mine", "Greyrock Mine"],
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
            per_population: 100,
        },
        BusinessTemplate {
            names: &["Forgeheart Steelworks", "Emberline Steelworks"],
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
            per_population: 100,
        },
        BusinessTemplate {
            names: &["Anvil & Edge Toolworks", "Keenblade Toolworks"],
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
            per_population: 100,
        },
        BusinessTemplate {
            names: &["Tallpine Lumber Camp", "Foxwood Lumber Camp"],
            kind: BusinessKind::LumberCamp,
            recipe: Recipe {
                inputs: vec![],
                output: (Good::Wood, 1),
                batches_per_worker: 2,
            },
            target_headcount: 1,
            wage: Money::from_cents(600),
            price: Money::from_cents(500),
            stock: &[(Good::Wood, 6)],
            sales_ema_milli: 1_000,
            uses_tools: true,
            per_population: 100,
        },
        BusinessTemplate {
            names: &["Redclay Brickworks", "Kilnford Brickworks"],
            kind: BusinessKind::Brickworks,
            recipe: Recipe {
                inputs: vec![],
                output: (Good::Bricks, 1),
                batches_per_worker: 2,
            },
            target_headcount: 1,
            wage: Money::from_cents(600),
            price: Money::from_cents(600),
            stock: &[(Good::Bricks, 6)],
            sales_ema_milli: 1_000,
            uses_tools: true,
            per_population: 100,
        },
        BusinessTemplate {
            names: &["Keystone Construction", "Archline Construction"],
            kind: BusinessKind::ConstructionCo,
            recipe: Recipe {
                inputs: vec![(Good::Wood, 6), (Good::Bricks, 6)],
                output: (Good::Home, 1),
                batches_per_worker: 1,
            },
            target_headcount: 1,
            wage: Money::from_cents(600),
            price: Money::from_cents(30_000),
            stock: &[(Good::Wood, 6), (Good::Bricks, 6), (Good::Home, 1)],
            sales_ema_milli: 100,
            uses_tools: false,
            per_population: 100,
        },
    ]
}

pub fn generate(config: WorldConfig) -> World {
    let mut rng = substream(config.master_seed, "worldgen", 0, 0);
    let templates = templates();
    // Population must cover one owner per business instance plus at least
    // one worker; instance counts themselves scale with population, so
    // clamp to the fixed point (instance counts grow far slower than
    // population, so this converges immediately).
    let mut population = config.population.max(2);
    loop {
        let business_count: u32 = templates.iter().map(|t| t.instances(population)).sum();
        if population > business_count {
            break;
        }
        population = business_count + 1;
    }

    let mut agents: BTreeMap<AgentId, Agent> = BTreeMap::new();
    for i in 0..population {
        let first = FIRST_NAMES[rng.gen_range(0..FIRST_NAMES.len())];
        let last = LAST_NAMES[rng.gen_range(0..LAST_NAMES.len())];
        let id = AgentId(i);
        // Personality from a dedicated per-agent substream: rolls in Traits
        // field order; adding features elsewhere never reshuffles who
        // someone is (DECISIONS.md #002 discipline).
        let mut trng = substream(config.master_seed, "traits", u64::from(i), 0);
        let mut roll = || trng.gen_range(0..=100u8);
        let traits = Traits {
            risk_tolerance: roll(),
            time_preference: roll(),
            loyalty: roll(),
            honesty: roll(),
            ambition: roll(),
            aggression: roll(),
            patience: roll(),
            empathy: roll(),
            greed: roll(),
        };
        agents.insert(
            id,
            Agent {
                id,
                name: format!("{first} {last}"),
                cash: START_AGENT_CASH,
                pantry: START_PANTRY,
                employer: None,
                owns: None,
                owns_home: false,
                traits,
                hungry_streak: 0,
                days_unemployed: 0,
                total_earned: Money::ZERO,
                total_spent: Money::ZERO,
            },
        );
    }

    let mut businesses: BTreeMap<BusinessId, Business> = BTreeMap::new();
    // Expand templates into instances in template order — ids, owners and
    // worker assignment stay deterministic and reproduce the audited
    // 29-person town exactly at the default population.
    let expanded: Vec<(usize, u32)> = templates
        .iter()
        .enumerate()
        .flat_map(|(ti, t)| (0..t.instances(population)).map(move |i| (ti, i)))
        .collect();
    let mut next_worker = expanded.len() as u32; // agents after the owners
    for (bi, (ti, inst)) in expanded.iter().enumerate() {
        let t = &templates[*ti];
        let bid = BusinessId(bi as u32);
        let owner = AgentId(bi as u32);
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
                name: t.instance_name(*inst),
                kind: t.kind,
                owner,
                cash: START_BUSINESS_CASH,
                workers,
                target_headcount: t.target_headcount,
                wage: t.wage,
                inventory,
                sells,
                price: t.price,
                recipe: t.recipe.clone(),
                books: Books::new(START_BUSINESS_CASH),
                uses_tools: t.uses_tools,
                tool_wear: 0,
                sales_ema_milli: t.sales_ema_milli,
                stockout_days: 0,
                dry_windows: 0,
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
        assert_eq!(w.state.agents.len(), 29);
        assert_eq!(w.state.businesses.len(), 10);
        let owners = w.state.agents.values().filter(|a| a.owns.is_some()).count();
        assert_eq!(owners, 10);
        let employed = w
            .state
            .agents
            .values()
            .filter(|a| a.employer.is_some())
            .count();
        assert_eq!(
            employed, 19,
            "3+3+3+4 food, 1+1+1 industry, 1+1+1 construction"
        );
        let tool_users = w.state.businesses.values().filter(|b| b.uses_tools).count();
        assert_eq!(tool_users, 5, "farms, mine, lumber camp, brickworks");
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
        // Clamped to one owner per minimum business set plus one worker.
        assert_eq!(w.state.agents.len(), 10);
        assert_eq!(w.state.businesses.len(), 9);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn traits_are_deterministic_and_vary_across_agents() {
        let a = generate(WorldConfig::default_with_seed(42));
        let b = generate(WorldConfig::default_with_seed(42));
        let first = *a.state.agents.keys().next().unwrap();
        assert_eq!(
            a.state.agents[&first].traits, b.state.agents[&first].traits,
            "same seed, same person"
        );
        let distinct: std::collections::BTreeSet<u8> =
            a.state.agents.values().map(|ag| ag.traits.greed).collect();
        assert!(distinct.len() > 3, "greed must vary across the town");
        let c = generate(WorldConfig::default_with_seed(43));
        assert_ne!(
            a.state.agents[&first].traits, c.state.agents[&first].traits,
            "different seed, different person (overwhelmingly likely)"
        );
    }

    #[test]
    fn scaling_yields_a_twenty_business_town_at_one_hundred() {
        let w = generate(WorldConfig {
            master_seed: 42,
            population: 100,
            hash_every: 50,
        });
        assert_eq!(w.state.agents.len(), 100);
        assert_eq!(
            w.state.businesses.len(),
            20,
            "7 farms + 3 mills + 4 bakeries + 6 single shops"
        );
        let farms = w
            .state
            .businesses
            .values()
            .filter(|b| b.kind == BusinessKind::Farm)
            .count();
        assert_eq!(farms, 7);
        let names: std::collections::BTreeSet<String> = w
            .state
            .businesses
            .values()
            .map(|b| b.name.clone())
            .collect();
        assert_eq!(names.len(), 20, "every instance gets a distinct name");
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }
}
