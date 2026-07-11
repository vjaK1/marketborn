//! Consumption phase: every agent eats one unit of food from the pantry per
//! day, or goes hungry (tracked and reported).

use crate::events::Event;
use crate::metrics::DayAccumulator;
use crate::world::{Journal, SimState};

pub fn run(state: &mut SimState, journal: &mut Journal, tick: u64, acc: &mut DayAccumulator) {
    for a in state.agents.values_mut() {
        if a.pantry >= 1 {
            a.pantry -= 1;
            a.hungry_streak = 0;
            acc.food_consumed += 1;
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
    use crate::worldgen::WorldConfig;
    use crate::World;

    #[test]
    fn agents_eat_from_pantry_or_go_hungry() {
        let mut w = World::from_config(WorldConfig::default_with_seed(2));
        let first = *w.state.agents.keys().next().unwrap();
        let last = *w.state.agents.keys().last().unwrap();
        w.state.agents.get_mut(&first).unwrap().pantry = 2;
        w.state.agents.get_mut(&last).unwrap().pantry = 0;
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
    }
}
