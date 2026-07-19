//! Consumption phase: every agent eats one unit of food from the pantry per
//! day (burned through the goods ledger), or goes hungry (tracked and
//! reported). Wealthy agents take a second, comfort meal — the channel that
//! returns idle household savings to circulation (DECISIONS.md #014).

use crate::events::Event;
use crate::goods_ledger;
use crate::ids::AgentId;
use crate::metrics::DayAccumulator;
use crate::money::Money;
use crate::world::{Journal, SimState};

/// Cash floor above which a household consumes (and shops for) a second
/// daily meal. Above everyone's starting cash so worldgen causes no day-one
/// demand shock; low enough that steady savers reach it within a season.
pub const COMFORT_CASH_FLOOR: Money = Money::from_cents(40_000);

pub fn run(state: &mut SimState, journal: &mut Journal, tick: u64, acc: &mut DayAccumulator) {
    let ids: Vec<AgentId> = state.agents.keys().copied().collect();
    for aid in ids {
        let (ate, comfort) = {
            let Some(a) = state.agents.get(&aid) else {
                continue;
            };
            // A comfort meal never causes hunger: it is taken only when a
            // meal would still remain in the pantry after the first.
            (a.pantry >= 1, a.pantry >= 2 && a.cash >= COMFORT_CASH_FLOOR)
        };
        if ate {
            let meals = if comfort { 2 } else { 1 };
            goods_ledger::consume_pantry(state, aid, meals);
            acc.food_consumed += meals;
        }
        let Some(a) = state.agents.get_mut(&aid) else {
            continue;
        };
        if ate {
            a.hungry_streak = 0;
        } else {
            a.hungry_streak += 1;
            acc.hungry_agents += 1;
            if a.hungry_streak == 1 || a.hungry_streak.is_multiple_of(7) {
                journal.push_event(
                    tick,
                    Event::AgentHungry {
                        agent: a.id,
                        streak: a.hungry_streak,
                    },
                );
            }
        }
        if a.is_job_seeker() {
            a.days_unemployed += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goods::Good;
    use crate::worldgen::WorldConfig;
    use crate::World;

    #[test]
    fn agents_eat_from_pantry_or_go_hungry() {
        let mut w = World::from_config(WorldConfig::default_with_seed(2));
        let first = *w.state.agents.keys().next().unwrap();
        let last = *w.state.agents.keys().last().unwrap();
        w.state.agents.get_mut(&first).unwrap().pantry = 2;
        w.state.agents.get_mut(&last).unwrap().pantry = 0;
        // Resync the conservation target after the out-of-band pantry edits.
        let food_total = w.state.total_goods(Good::Food);
        w.state.expected_total_goods.insert(Good::Food, food_total);
        let mut acc = DayAccumulator::default();
        run(&mut w.state, &mut w.journal, 1, &mut acc);
        assert_eq!(w.state.agents[&first].pantry, 1);
        assert_eq!(w.state.agents[&first].hungry_streak, 0);
        assert_eq!(w.state.agents[&last].hungry_streak, 1);
        assert_eq!(acc.hungry_agents, 1);
        let hungry_events = w
            .journal
            .events
            .iter()
            .filter(|e| matches!(e.event, Event::AgentHungry { .. }))
            .count();
        assert_eq!(hungry_events, 1, "streak day 1 emits one event");
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn wealthy_agents_take_a_comfort_meal_but_never_into_hunger() {
        let mut w = World::from_config(WorldConfig::default_with_seed(2));
        let ids: Vec<_> = w.state.agents.keys().copied().take(3).collect();
        {
            let a = w.state.agents.get_mut(&ids[0]).unwrap();
            a.cash = COMFORT_CASH_FLOOR; // rich, well stocked: eats twice
            a.pantry = 3;
        }
        {
            let a = w.state.agents.get_mut(&ids[1]).unwrap();
            a.cash = COMFORT_CASH_FLOOR; // rich, last meal: comfort declined
            a.pantry = 1;
        }
        {
            let a = w.state.agents.get_mut(&ids[2]).unwrap();
            a.cash = Money::from_cents(100); // poor: one meal
            a.pantry = 3;
        }
        w.state.expected_total_money = w.state.total_cash();
        let food_total = w.state.total_goods(Good::Food);
        w.state.expected_total_goods.insert(Good::Food, food_total);
        let mut acc = DayAccumulator::default();
        run(&mut w.state, &mut w.journal, 1, &mut acc);
        assert_eq!(w.state.agents[&ids[0]].pantry, 1, "comfort meal eaten");
        assert_eq!(w.state.agents[&ids[1]].pantry, 0, "one meal, no hunger");
        assert_eq!(w.state.agents[&ids[1]].hungry_streak, 0);
        assert_eq!(w.state.agents[&ids[2]].pantry, 2, "poor eat once");
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }
}
