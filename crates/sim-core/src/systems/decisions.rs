//! Decisions phase (Phase 0 scope: business owners).
//!
//! Daily: emergency downsizing when cash cannot cover a two-day payroll.
//! Every seven days, staggered by business id: price review (stockouts push
//! prices up, gluts push them down, bounded steps), wage review, dividend.
//! The full utility-scored agent decision engine arrives in project Phase 2.

use crate::events::Event;
use crate::goods::Qty;
use crate::ids::{AccountId, AgentId, BusinessId};
use crate::ledger::{self, LedgerError, TxKind};
use crate::market;
use crate::money::Money;
use crate::world::{Journal, SimState};

pub const REVIEW_PERIOD: u64 = 7;
const EMERGENCY_PAYROLL_DAYS: i64 = 2;
/// Owners keep this much personal cash before recapitalizing a business.
const OWNER_RESERVE: Money = Money::from_cents(10_000);
const PRICE_RAISE_BP: i64 = 700;
const PRICE_CUT_HEAVY_BP: i64 = 500;
const PRICE_CUT_LIGHT_BP: i64 = 200;
const WAGE_RAISE_BP: i64 = 500;
const WAGE_CUT_BP: i64 = 300;
const DIVIDEND_BP: i64 = 2500;
const PRICE_FLOOR: Money = Money::from_cents(10);
/// Mechanical sanity bound on posted prices ($100,000/unit): prevents
/// numeric absurdity if scarcity persists for years. Documented in
/// ECONOMIC_RULES.md §Decisions.
const PRICE_CEILING: Money = Money::from_cents(10_000_000);
const WAGE_FLOOR: Money = Money::from_cents(300);
const WAGE_CEILING: Money = Money::from_cents(1_000_000);
const DIVIDEND_BUFFER_PAYROLL_DAYS: i64 = 21;
const STOCKOUT_RAISE_THRESHOLD: u32 = 2;
const GLUT_HEAVY_DAYS: Qty = 8;
const GLUT_LIGHT_DAYS: Qty = 5;

struct ReviewPlan {
    window_profit: Money,
    new_price: Option<(Money, Money)>,
    new_wage: Option<(Money, Money)>,
    dividend: Option<(AgentId, Money)>,
}

pub fn run(state: &mut SimState, journal: &mut Journal, tick: u64) -> Result<(), LedgerError> {
    let business_ids: Vec<BusinessId> = state.businesses.keys().copied().collect();
    for bid in business_ids {
        // --- Daily: owner capital injection. A business too broke to fund a
        // single hire draws on its owner's savings above a personal reserve —
        // the Phase 0 slice of the brief's "invest" action, and the channel
        // that returns household money to production after a bust. ---
        let inject: Option<(AgentId, Money)> = {
            let Some(b) = state.businesses.get(&bid) else {
                continue;
            };
            let hire_floor = b
                .wage
                .checked_mul_qty(crate::systems::labor::HIRING_CASH_DAYS)
                .unwrap_or(Money::MAX);
            if b.cash >= hire_floor {
                None
            } else {
                let owner_cash = state
                    .agents
                    .get(&b.owner)
                    .map(|a| a.cash)
                    .unwrap_or(Money::ZERO);
                let available = owner_cash - OWNER_RESERVE;
                // Fund up to two workers for the hiring window.
                let want = hire_floor.checked_mul_qty(2).unwrap_or(Money::MAX) - b.cash;
                let amount = want.min(available);
                if amount > Money::ZERO {
                    Some((b.owner, amount))
                } else {
                    None
                }
            }
        };
        if let Some((owner, amount)) = inject {
            ledger::transfer(
                state,
                journal,
                tick,
                AccountId::Agent(owner),
                AccountId::Business(bid),
                amount,
                TxKind::OwnerInvestment,
            )?;
            journal.push_event(
                tick,
                Event::OwnerInvested {
                    business: bid,
                    owner,
                    amount,
                },
            );
        }

        // --- Daily: emergency downsizing (last hired leaves first). ---
        let fire: Option<AgentId> = {
            let Some(b) = state.businesses.get(&bid) else {
                continue;
            };
            let two_days = b
                .daily_payroll()
                .checked_mul_qty(EMERGENCY_PAYROLL_DAYS)
                .unwrap_or(Money::MAX);
            if !b.workers.is_empty() && b.cash < two_days {
                b.workers.last().copied()
            } else {
                None
            }
        };
        if let Some(aid) = fire {
            if let Some(b) = state.businesses.get_mut(&bid) {
                b.workers.pop();
            }
            if let Some(a) = state.agents.get_mut(&aid) {
                a.employer = None;
            }
            journal.push_event(
                tick,
                Event::Fired {
                    agent: aid,
                    business: bid,
                },
            );
        }

        // --- Weekly review, staggered by id so competitors never all move
        // on the same day. ---
        if !(tick + bid.0 as u64).is_multiple_of(REVIEW_PERIOD) {
            continue;
        }
        let plan: ReviewPlan = {
            let Some(b) = state.businesses.get(&bid) else {
                continue;
            };
            let ema_day = b.expected_daily_sales();
            let stock = b.stock(b.sells);
            let min_step = Money::from_cents(1);

            let mut new_price = None;
            if b.stockout_days >= STOCKOUT_RAISE_THRESHOLD {
                let raised =
                    (b.price + b.price.mul_bp(PRICE_RAISE_BP).max(min_step)).min(PRICE_CEILING);
                if raised != b.price {
                    new_price = Some((b.price, raised));
                }
            } else if stock > ema_day * GLUT_HEAVY_DAYS {
                let cut =
                    (b.price - b.price.mul_bp(PRICE_CUT_HEAVY_BP).max(min_step)).max(PRICE_FLOOR);
                if cut != b.price {
                    new_price = Some((b.price, cut));
                }
            } else if stock > ema_day * GLUT_LIGHT_DAYS {
                let cut =
                    (b.price - b.price.mul_bp(PRICE_CUT_LIGHT_BP).max(min_step)).max(PRICE_FLOOR);
                if cut != b.price {
                    new_price = Some((b.price, cut));
                }
            }

            let window_profit = b.revenue_window - b.costs_window;
            let mut new_wage = None;
            // Raise wages to attract labor only from a position of strength:
            // a loss-making business bidding wages up while broke is the
            // death-spiral input, not competition.
            if b.vacancies() > 0
                && b.vacancy_days >= REVIEW_PERIOD as u32
                && !window_profit.is_negative()
            {
                let raised =
                    (b.wage + b.wage.mul_bp(WAGE_RAISE_BP).max(min_step)).min(WAGE_CEILING);
                if raised != b.wage {
                    new_wage = Some((b.wage, raised));
                }
            } else if b.vacancies() == 0 && window_profit.is_negative() && b.wage > WAGE_FLOOR {
                let cut = (b.wage - b.wage.mul_bp(WAGE_CUT_BP).max(min_step)).max(WAGE_FLOOR);
                if cut != b.wage {
                    new_wage = Some((b.wage, cut));
                }
            }

            // Dividend: pay out a quarter of cash above a survival buffer
            // (three weeks of payroll plus a week of input purchases at last
            // observed prices). The rounding remainder — and the other 75% —
            // stays with the business.
            let mut input_week = Money::ZERO;
            for (good, _) in &b.recipe.inputs {
                let need = market::daily_input_need(b, *good);
                let price = state
                    .market
                    .last_prices
                    .get(good)
                    .copied()
                    .unwrap_or(Money::ZERO);
                input_week += price.checked_mul_qty(need * 7).unwrap_or(Money::ZERO);
            }
            // Buffer against the *target* headcount, not the current one, so
            // a downsized business retains the capital to staff back up
            // instead of paying it out.
            let target_payroll = b
                .wage
                .checked_mul_qty(b.target_headcount as i64)
                .unwrap_or(Money::MAX);
            let buffer = target_payroll
                .checked_mul_qty(DIVIDEND_BUFFER_PAYROLL_DAYS)
                .unwrap_or(Money::MAX)
                + input_week;
            let dividend = if b.cash > buffer {
                let pay = (b.cash - buffer).mul_bp(DIVIDEND_BP);
                if pay > Money::ZERO {
                    Some((b.owner, pay))
                } else {
                    None
                }
            } else {
                None
            };

            ReviewPlan {
                window_profit,
                new_price,
                new_wage,
                dividend,
            }
        };

        let sells = state.businesses.get(&bid).map(|b| b.sells);
        if let Some(b) = state.businesses.get_mut(&bid) {
            b.last_window_profit = plan.window_profit;
            b.revenue_window = Money::ZERO;
            b.costs_window = Money::ZERO;
            b.stockout_days = 0;
            b.vacancy_days = 0;
            if let Some((_, new)) = plan.new_price {
                b.price = new;
            }
            if let Some((_, new)) = plan.new_wage {
                b.wage = new;
            }
        }
        if let (Some((old, new)), Some(good)) = (plan.new_price, sells) {
            journal.push_event(
                tick,
                Event::PriceChanged {
                    business: bid,
                    good,
                    old,
                    new,
                },
            );
        }
        if let Some((old, new)) = plan.new_wage {
            journal.push_event(
                tick,
                Event::WageChanged {
                    business: bid,
                    old,
                    new,
                },
            );
        }
        if let Some((owner, amount)) = plan.dividend {
            ledger::transfer(
                state,
                journal,
                tick,
                AccountId::Business(bid),
                AccountId::Agent(owner),
                amount,
                TxKind::Dividend,
            )?;
            if let Some(a) = state.agents.get_mut(&owner) {
                a.total_earned += amount;
            }
            journal.push_event(
                tick,
                Event::DividendPaid {
                    business: bid,
                    owner,
                    amount,
                },
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldConfig;
    use crate::World;

    /// The review tick for a business id under the (tick + id) % 7 stagger.
    fn review_tick_for(bid: BusinessId) -> u64 {
        let rem = bid.0 as u64 % REVIEW_PERIOD;
        (REVIEW_PERIOD - rem) % REVIEW_PERIOD + REVIEW_PERIOD
    }

    #[test]
    fn stockouts_raise_prices() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let bid = *w.state.businesses.keys().next().unwrap();
        let t = review_tick_for(bid);
        let old = {
            let b = w.state.businesses.get_mut(&bid).unwrap();
            b.stockout_days = 3;
            b.inventory.clear();
            b.price
        };
        run(&mut w.state, &mut w.journal, t).unwrap();
        let b = &w.state.businesses[&bid];
        assert!(b.price > old, "stockout must raise price");
        assert_eq!(b.stockout_days, 0, "review resets the window");
    }

    #[test]
    fn glut_cuts_prices_toward_floor() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let bid = *w.state.businesses.keys().next().unwrap();
        let t = review_tick_for(bid);
        let old = {
            let b = w.state.businesses.get_mut(&bid).unwrap();
            b.stockout_days = 0;
            let sells = b.sells;
            b.sales_ema_milli = 1_000; // expects 1/day
            b.add_stock(sells, 100); // massive glut
            b.price
        };
        run(&mut w.state, &mut w.journal, t).unwrap();
        assert!(w.state.businesses[&bid].price < old, "glut must cut price");
        assert!(w.state.businesses[&bid].price >= PRICE_FLOOR);
    }

    #[test]
    fn cash_crunch_fires_last_hired_daily() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let bid = *w.state.businesses.keys().next().unwrap();
        let last_hired = *w.state.businesses[&bid].workers.last().unwrap();
        let staff_before = w.state.businesses[&bid].workers.len();
        let owner = w.state.businesses[&bid].owner;
        w.state.businesses.get_mut(&bid).unwrap().cash = Money::from_cents(100);
        // The owner is broke too, so no capital injection can rescue payroll.
        w.state.agents.get_mut(&owner).unwrap().cash = Money::from_cents(500);
        w.state.expected_total_money = w.state.total_cash();
        // Non-review tick: only the daily emergency rule runs.
        let t = review_tick_for(bid) + 1;
        run(&mut w.state, &mut w.journal, t).unwrap();
        let b = &w.state.businesses[&bid];
        assert_eq!(b.workers.len(), staff_before - 1);
        assert_eq!(w.state.agents[&last_hired].employer, None);
    }

    #[test]
    fn broke_owner_with_savings_recapitalizes_business() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let bid = *w.state.businesses.keys().next().unwrap();
        let owner = w.state.businesses[&bid].owner;
        w.state.businesses.get_mut(&bid).unwrap().cash = Money::from_cents(100);
        w.state.expected_total_money = w.state.total_cash();
        let owner_cash_before = w.state.agents[&owner].cash;
        let t = review_tick_for(bid) + 1; // non-review day: daily rules only
        run(&mut w.state, &mut w.journal, t).unwrap();
        let b = &w.state.businesses[&bid];
        assert!(
            b.cash > Money::from_cents(100),
            "owner savings must flow into the business"
        );
        assert!(w.state.agents[&owner].cash < owner_cash_before);
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(e.event, Event::OwnerInvested { business, .. } if business == bid)));
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    }

    #[test]
    fn rich_business_pays_dividend_to_owner() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let bid = *w.state.businesses.keys().next().unwrap();
        let owner = w.state.businesses[&bid].owner;
        let owner_cash_before = w.state.agents[&owner].cash;
        {
            let b = w.state.businesses.get_mut(&bid).unwrap();
            b.cash = Money::from_cents(10_000_000);
        }
        w.state.expected_total_money = w.state.total_cash();
        let t = review_tick_for(bid);
        run(&mut w.state, &mut w.journal, t).unwrap();
        assert!(
            w.state.agents[&owner].cash > owner_cash_before,
            "owner must receive a dividend"
        );
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    }
}
