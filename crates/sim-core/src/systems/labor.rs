//! Labor phase: weekly job reviews (Phase 2 mobility), deterministic job
//! matching, then daily payroll.
//!
//! Job reviews (per-agent stagger): an employed worker switches to an open
//! job whose wage clears a loyalty-widened premium over the current one;
//! a comfortable unemployed agent holds out above an ambition-scaled
//! reservation wage (desperation — hunger or thin savings — drops it to
//! zero). Matching: businesses in id order fill vacancies from willing job
//! seekers in id order, provided the business holds a hiring cash buffer.
//! Payroll: every worker is paid the posted daily wage in hire order;
//! workers the business cannot pay quit immediately (and the miss is a
//! public event).

use crate::decision::{self, DecisionDetail, JobAction, JobRef};
use crate::events::Event;
use crate::goods::Good;
use crate::ids::{AccountId, AgentId, BusinessId};
use crate::ledger::{self, LedgerError, TxKind};
use crate::money::Money;
use crate::world::{Journal, SimState};

/// A business only hires while it can cover this many days of payroll at
/// full target headcount.
pub const HIRING_CASH_DAYS: i64 = 5;
/// Per-agent job-review stagger (same period as business reviews).
const JOB_REVIEW_PERIOD: u64 = 7;
/// An unemployed agent is desperate below this many days of food savings.
const DESPERATION_FOOD_DAYS: i64 = 30;
/// Fallback food price before any market trade exists.
const FALLBACK_FOOD_PRICE: Money = Money::from_cents(540);

fn going_food_price(state: &SimState) -> Money {
    state
        .market
        .last_prices
        .get(&Good::Food)
        .copied()
        .unwrap_or(FALLBACK_FOOD_PRICE)
}

/// The best open job right now: a vacancy whose business passes the same
/// marginal cash gate hiring uses. Highest wage wins; ties go to the lower
/// business id.
fn best_open_job(state: &SimState, exclude: Option<BusinessId>) -> Option<JobRef> {
    let mut best: Option<JobRef> = None;
    for b in state.businesses.values() {
        if b.vacancies() == 0 || Some(b.id) == exclude {
            continue;
        }
        let five_days_one_worker = b
            .wage
            .checked_mul_qty(HIRING_CASH_DAYS)
            .unwrap_or(Money::MAX);
        if b.cash.affordable_units(five_days_one_worker) <= b.workers.len() as i64 {
            continue;
        }
        let better = match best {
            None => true,
            Some(cur) => b.wage > cur.wage,
        };
        if better {
            best = Some(JobRef {
                business: b.id,
                wage: b.wage,
            });
        }
    }
    best
}

fn is_desperate(a: &crate::agent::Agent, food_price: Money) -> bool {
    let month_of_food = food_price
        .checked_mul_qty(DESPERATION_FOOD_DAYS)
        .unwrap_or(Money::MAX);
    a.hungry_streak > 0 || a.cash < month_of_food
}

/// An executed switch: (from, to, old wage, new wage).
type SwitchExec = (BusinessId, BusinessId, Money, Money);

/// Weekly job reviews, in agent id order. Switches execute immediately, so
/// later reviewers see updated rosters — deterministic by construction.
fn job_reviews(state: &mut SimState, journal: &mut Journal, tick: u64) {
    let food_price = going_food_price(state);
    let agent_ids: Vec<AgentId> = state.agents.keys().copied().collect();
    for aid in agent_ids {
        if !(tick + u64::from(aid.0)).is_multiple_of(JOB_REVIEW_PERIOD) {
            continue;
        }
        let plan: Option<(DecisionDetail, Option<SwitchExec>)> = {
            let Some(a) = state.agents.get(&aid) else {
                continue;
            };
            if a.owns.is_some() {
                continue; // owners run their business; they don't job-hunt
            }
            match a.employer {
                Some(current_bid) => {
                    let current_wage = state
                        .businesses
                        .get(&current_bid)
                        .map(|b| b.wage)
                        .unwrap_or(Money::ZERO);
                    let best = best_open_job(state, Some(current_bid));
                    let premium_bp = decision::switch_premium_bp(a.traits.loyalty);
                    let bar = current_wage + current_wage.mul_bp(premium_bp);
                    match best {
                        Some(offer) if offer.wage >= bar => Some((
                            DecisionDetail::JobReview {
                                current: Some(JobRef {
                                    business: current_bid,
                                    wage: current_wage,
                                }),
                                best_open: best,
                                reservation: Money::ZERO,
                                premium_bp,
                                chosen: JobAction::SwitchTo(offer.business),
                            },
                            Some((current_bid, offer.business, current_wage, offer.wage)),
                        )),
                        Some(_) => Some((
                            DecisionDetail::JobReview {
                                current: Some(JobRef {
                                    business: current_bid,
                                    wage: current_wage,
                                }),
                                best_open: best,
                                reservation: Money::ZERO,
                                premium_bp,
                                chosen: JobAction::Stay,
                            },
                            None,
                        )),
                        None => None, // nothing open: no record, no noise
                    }
                }
                None => {
                    let best = best_open_job(state, None);
                    let desperate = is_desperate(a, food_price);
                    let reservation = decision::reservation_wage(
                        food_price,
                        a.traits.ambition,
                        a.traits.patience,
                        a.days_unemployed,
                        desperate,
                    );
                    match best {
                        // Holding out is a real decision worth recording;
                        // acceptable offers are taken by the matching pass.
                        Some(offer) if offer.wage < reservation => Some((
                            DecisionDetail::JobReview {
                                current: None,
                                best_open: best,
                                reservation,
                                premium_bp: 0,
                                chosen: JobAction::HoldOut,
                            },
                            None,
                        )),
                        _ => None,
                    }
                }
            }
        };
        let Some((detail, switch)) = plan else {
            continue;
        };
        journal.push_decision(decision::DecisionRecord {
            seq: 0,
            tick,
            actor: aid,
            detail,
        });
        if let Some((from, to, old_wage, new_wage)) = switch {
            if let Some(b) = state.businesses.get_mut(&from) {
                b.workers.retain(|w| *w != aid);
            }
            if let Some(b) = state.businesses.get_mut(&to) {
                b.workers.push(aid);
            }
            if let Some(a) = state.agents.get_mut(&aid) {
                a.employer = Some(to);
            }
            journal.push_event(
                tick,
                Event::JobSwitched {
                    agent: aid,
                    from,
                    to,
                    old_wage,
                    new_wage,
                },
            );
        }
    }
}

pub fn run(state: &mut SimState, journal: &mut Journal, tick: u64) -> Result<(), LedgerError> {
    job_reviews(state, journal, tick);
    let food_price = going_food_price(state);
    let business_ids: Vec<BusinessId> = state.businesses.keys().copied().collect();

    // --- Matching ---
    for bid in &business_ids {
        let (vacancies, wage, affordable_headcount) = {
            let Some(b) = state.businesses.get(bid) else {
                continue;
            };
            // Marginal hiring gate: staff up only as far as cash can cover
            // HIRING_CASH_DAYS of payroll for the resulting headcount. A
            // downsized business can bootstrap back one worker at a time.
            let five_days_one_worker = b
                .wage
                .checked_mul_qty(HIRING_CASH_DAYS)
                .unwrap_or(Money::MAX);
            let affordable = b.cash.affordable_units(five_days_one_worker);
            (b.vacancies() as usize, b.wage, affordable)
        };
        let current = state
            .businesses
            .get(bid)
            .map(|b| b.workers.len() as i64)
            .unwrap_or(0);
        let hire_count = (affordable_headcount - current).clamp(0, vacancies as i64) as usize;
        if hire_count == 0 {
            continue;
        }
        let hires: Vec<AgentId> = state
            .agents
            .values()
            .filter(|a| a.is_job_seeker())
            .filter(|a| {
                let reservation = decision::reservation_wage(
                    food_price,
                    a.traits.ambition,
                    a.traits.patience,
                    a.days_unemployed,
                    is_desperate(a, food_price),
                );
                wage >= reservation
            })
            .map(|a| a.id)
            .take(hire_count)
            .collect();
        for aid in hires {
            if let Some(a) = state.agents.get_mut(&aid) {
                a.employer = Some(*bid);
                a.days_unemployed = 0;
            }
            if let Some(b) = state.businesses.get_mut(bid) {
                b.workers.push(aid);
            }
            journal.push_event(
                tick,
                Event::Hired {
                    agent: aid,
                    business: *bid,
                    wage,
                },
            );
        }
    }

    // --- Vacancy aging (read by the wage review) ---
    for b in state.businesses.values_mut() {
        if b.vacancies() > 0 {
            b.vacancy_days += 1;
        }
    }

    // --- Payroll ---
    for bid in &business_ids {
        let (workers, wage) = {
            let Some(b) = state.businesses.get(bid) else {
                continue;
            };
            (b.workers.clone(), b.wage)
        };
        if workers.is_empty() {
            continue;
        }
        let mut unpaid: Vec<AgentId> = Vec::new();
        let mut paid_total = Money::ZERO;
        for aid in workers {
            match ledger::transfer(
                state,
                journal,
                tick,
                AccountId::Business(*bid),
                AccountId::Agent(aid),
                wage,
                TxKind::Wage,
            ) {
                Ok(()) => {
                    if let Some(a) = state.agents.get_mut(&aid) {
                        a.total_earned += wage;
                    }
                    paid_total += wage;
                }
                Err(LedgerError::InsufficientFunds { .. }) => unpaid.push(aid),
                Err(e) => return Err(e),
            }
        }
        if let Some(b) = state.businesses.get_mut(bid) {
            b.costs_window += paid_total;
            b.books.wages += paid_total;
            if unpaid.is_empty() {
                b.missed_payroll_days = 0;
            } else {
                b.missed_payroll_days += 1;
            }
        }
        if !unpaid.is_empty() {
            let shortfall = wage
                .checked_mul_qty(unpaid.len() as i64)
                .unwrap_or(Money::MAX);
            journal.push_event(
                tick,
                Event::MissedPayroll {
                    business: *bid,
                    workers_unpaid: unpaid.len() as u32,
                    shortfall,
                },
            );
            for aid in unpaid {
                if let Some(b) = state.businesses.get_mut(bid) {
                    b.workers.retain(|w| *w != aid);
                }
                if let Some(a) = state.agents.get_mut(&aid) {
                    a.employer = None;
                }
                journal.push_event(
                    tick,
                    Event::QuitUnpaid {
                        agent: aid,
                        business: *bid,
                        owed: wage,
                    },
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldConfig;
    use crate::World;

    #[test]
    fn payroll_pays_every_worker_daily() {
        let mut w = World::from_config(WorldConfig::default_with_seed(11));
        let worker_cash_before: Vec<Money> = w
            .state
            .agents
            .values()
            .filter(|a| a.employer.is_some())
            .map(|a| a.cash)
            .collect();
        assert!(!worker_cash_before.is_empty());
        run(&mut w.state, &mut w.journal, 1).unwrap();
        let workers_after: Vec<&crate::Agent> = w
            .state
            .agents
            .values()
            .filter(|a| a.employer.is_some())
            .collect();
        for a in workers_after {
            assert!(a.cash > Money::from_cents(30_000), "{} was not paid", a.id);
        }
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    }

    #[test]
    fn broke_business_misses_payroll_and_loses_workers() {
        let mut w = World::from_config(WorldConfig::default_with_seed(11));
        let bid = *w.state.businesses.keys().next().unwrap();
        let staff_before = w.state.businesses[&bid].workers.len();
        assert!(staff_before > 0);
        w.state.businesses.get_mut(&bid).unwrap().cash = Money::ZERO;
        // Keep conservation intact for the test's bookkeeping.
        w.state.expected_total_money = w.state.total_cash();
        run(&mut w.state, &mut w.journal, 1).unwrap();
        let b = &w.state.businesses[&bid];
        assert!(b.workers.is_empty(), "unpaid workers must quit");
        assert_eq!(b.missed_payroll_days, 1);
        let quit_events = w
            .journal
            .events
            .iter()
            .filter(|e| matches!(e.event, Event::QuitUnpaid { business, .. } if business == bid))
            .count();
        assert_eq!(quit_events, staff_before);
    }

    #[test]
    fn matching_refills_vacancies_deterministically() {
        let mut w = World::from_config(WorldConfig::default_with_seed(11));
        let bid = *w.state.businesses.keys().next().unwrap();
        // Maximum loyalty everywhere and modest ambition: no rival ever
        // clears a switch premium, and no seeker holds out — this test is
        // about deterministic matching order, not mobility.
        for a in w.state.agents.values_mut() {
            a.traits.loyalty = 100;
            a.traits.ambition = 0;
        }
        // Fire one worker manually.
        let aid = w
            .state
            .businesses
            .get_mut(&bid)
            .unwrap()
            .workers
            .pop()
            .unwrap();
        w.state.agents.get_mut(&aid).unwrap().employer = None;
        let seekers_before: Vec<AgentId> = w
            .state
            .agents
            .values()
            .filter(|a| a.is_job_seeker())
            .map(|a| a.id)
            .collect();
        run(&mut w.state, &mut w.journal, 1).unwrap();
        // The lowest-id seeker got the job.
        let hired = seekers_before[0];
        assert_eq!(w.state.agents[&hired].employer, Some(bid));
        assert_eq!(
            w.state.businesses[&bid].vacancies(),
            0,
            "vacancy must be refilled"
        );
    }

    #[test]
    fn workers_switch_only_over_the_loyalty_premium() {
        // Farm worker id 10 (wage $7.00) reviews on tick 4; the bakery
        // (id 3) posts $8.00 with a vacancy. Bar: disloyal +10% = $7.70
        // (clears), loyal +20% = $8.40 (does not).
        for (loyalty, expect_switch) in [(0u8, true), (100u8, false)] {
            let mut w = World::from_config(WorldConfig::default_with_seed(11));
            let farm = *w.state.businesses.keys().next().unwrap();
            let bakery = crate::ids::BusinessId(3);
            let mover = AgentId(10);
            assert_eq!(w.state.agents[&mover].employer, Some(farm));
            {
                let b = w.state.businesses.get_mut(&bakery).unwrap();
                b.wage = Money::from_cents(800);
                let popped = b.workers.pop().unwrap();
                w.state.agents.get_mut(&popped).unwrap().employer = None;
                w.state.agents.get_mut(&popped).unwrap().traits.ambition = 0;
            }
            w.state.agents.get_mut(&mover).unwrap().traits.loyalty = loyalty;
            run(&mut w.state, &mut w.journal, 4).unwrap();
            let now = w.state.agents[&mover].employer;
            if expect_switch {
                assert_eq!(now, Some(bakery), "10% bar cleared: switch");
                assert!(w.journal.events.iter().any(
                    |e| matches!(e.event, Event::JobSwitched { agent, .. } if agent == mover)
                ));
            } else {
                assert_eq!(now, Some(farm), "20% bar not cleared: stay");
            }
        }
    }

    #[test]
    fn comfortable_ambition_holds_out_and_desperation_accepts() {
        let mut w = World::from_config(WorldConfig::default_with_seed(11));
        let farm = *w.state.businesses.keys().next().unwrap();
        // Freeze bystander mobility: this test is about the reservation
        // wage, not switching.
        for a in w.state.agents.values_mut() {
            a.traits.loyalty = 100;
        }
        // Free up one farm job; the freed worker (id 12) is ambitious and
        // comfortable: reservation 1.5 × $5.40 = $8.10 > the $7.00 wage.
        let seeker = w
            .state
            .businesses
            .get_mut(&farm)
            .unwrap()
            .workers
            .pop()
            .unwrap();
        {
            let a = w.state.agents.get_mut(&seeker).unwrap();
            a.employer = None;
            a.traits.ambition = 100;
        }
        // Tick 2 is id 12's review day ((2 + 12) % 7 == 0).
        assert_eq!(seeker, AgentId(12));
        run(&mut w.state, &mut w.journal, 2).unwrap();
        assert_eq!(w.state.agents[&seeker].employer, None, "held out");
        assert!(w.journal.decisions.iter().any(|d| {
            d.actor == seeker
                && matches!(
                    d.detail,
                    crate::decision::DecisionDetail::JobReview {
                        chosen: crate::decision::JobAction::HoldOut,
                        ..
                    }
                )
        }));
        // Broke, the same person takes the same job the next day.
        w.state.agents.get_mut(&seeker).unwrap().cash = Money::from_cents(100);
        w.state.expected_total_money = w.state.total_cash();
        run(&mut w.state, &mut w.journal, 3).unwrap();
        assert_eq!(
            w.state.agents[&seeker].employer,
            Some(farm),
            "desperation accepts"
        );
    }
}
