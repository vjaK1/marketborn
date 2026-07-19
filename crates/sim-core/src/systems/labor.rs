//! Labor phase: deterministic job matching, then daily payroll.
//!
//! Matching: businesses in id order fill vacancies from job seekers in id
//! order, provided the business holds a hiring cash buffer. Payroll: every
//! worker is paid the posted daily wage in hire order; workers the business
//! cannot pay quit immediately (and the miss is a public event).

use crate::events::Event;
use crate::ids::{AccountId, AgentId, BusinessId};
use crate::ledger::{self, LedgerError, TxKind};
use crate::money::Money;
use crate::world::{Journal, SimState};

/// A business only hires while it can cover this many days of payroll at
/// full target headcount.
pub const HIRING_CASH_DAYS: i64 = 5;

pub fn run(state: &mut SimState, journal: &mut Journal, tick: u64) -> Result<(), LedgerError> {
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
}
