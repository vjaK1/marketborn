//! Posted-price goods markets.
//!
//! Each good has standing sell offers posted by businesses (rebuilt each tick
//! from current inventory and posted prices — equivalent to refreshing a
//! standing registry). Buyers purchase in deterministic order — urgency tier
//! first, then account order (businesses before households, then by id) —
//! each taking the cheapest available offer (ties broken by lower seller id),
//! subject to available funds. Specified in `docs/ECONOMIC_RULES.md` §Markets.

use crate::business::Business;
use crate::goods::{Good, Qty};
use crate::ids::{AccountId, BusinessId};
use crate::ledger::{self, LedgerError, TxKind};
use crate::metrics::DayAccumulator;
use crate::money::Money;
use crate::world::{Journal, SimState};

/// How many days of input consumption a producer keeps on hand.
pub const INPUT_TARGET_DAYS: Qty = 3;
/// Households top the pantry up to this many days of food.
pub const PANTRY_TARGET: i64 = 3;
/// Days of payroll a business reserves before spending on inputs.
const PAYROLL_RESERVE_DAYS: i64 = 3;

#[derive(Clone, Copy, Debug)]
struct Offer {
    seller: BusinessId,
    price: Money,
    qty: Qty,
}

#[derive(Clone, Copy, Debug)]
struct Order {
    buyer: AccountId,
    /// 0 = urgent (starving household / stalled production), 1 = routine.
    urgency: u8,
    qty: Qty,
    /// Spending cap for this order (businesses protect payroll).
    /// `None` = limited only by cash on hand.
    max_spend: Option<Money>,
    /// Reservation price: the buyer refuses offers above this unit price.
    /// `None` = price-taker (households buying survival food).
    max_unit_price: Option<Money>,
}

/// Share of a batch's sales revenue a producer will spend on that batch's
/// inputs (basis points). The rest covers wages and margin. This is the
/// marginal-revenue cap that damps cost-push spirals: an upstream price
/// above what the output resells for finds no buyer, gluts, and falls.
const INPUT_REVENUE_SHARE_BP: i64 = 7000;

/// Units of input good consumed per day at the expected production rate.
pub fn daily_input_need(business: &Business, good: Good) -> Qty {
    let per_batch: Qty = business
        .recipe
        .inputs
        .iter()
        .filter(|(g, _)| *g == good)
        .map(|(_, q)| *q)
        .sum();
    if per_batch == 0 {
        return 0;
    }
    let out_per_batch = business.recipe.output.1.max(1);
    let batches_needed = (business.expected_daily_sales() + out_per_batch - 1) / out_per_batch;
    batches_needed * per_batch
}

fn build_offers(state: &SimState, good: Good) -> Vec<Offer> {
    let mut offers: Vec<Offer> = state
        .businesses
        .values()
        .filter(|b| b.sells == good && b.stock(good) > 0)
        .map(|b| Offer {
            seller: b.id,
            price: b.price,
            qty: b.stock(good),
        })
        .collect();
    offers.sort_by_key(|o| (o.price, o.seller));
    offers
}

fn build_orders(state: &SimState, good: Good) -> Vec<Order> {
    let mut orders: Vec<Order> = Vec::new();

    // Producers restock inputs toward INPUT_TARGET_DAYS of consumption.
    for b in state.businesses.values() {
        let need_per_day = daily_input_need(b, good);
        if need_per_day == 0 {
            continue;
        }
        let target = need_per_day * INPUT_TARGET_DAYS;
        let current = b.stock(good);
        let want = target - current;
        if want <= 0 {
            continue;
        }
        let reserve = b
            .wage
            .checked_mul_qty(b.workers.len() as i64 * PAYROLL_RESERVE_DAYS)
            .unwrap_or(Money::MAX);
        let budget = (b.cash - reserve).max(Money::ZERO);
        if budget == Money::ZERO {
            continue;
        }
        // Reservation price: revenue per batch × input share ÷ units of this
        // input per batch (integer division toward zero).
        let per_batch: Qty = b
            .recipe
            .inputs
            .iter()
            .filter(|(g, _)| *g == good)
            .map(|(_, q)| *q)
            .sum::<Qty>()
            .max(1);
        let revenue_per_batch = b
            .price
            .checked_mul_qty(b.recipe.output.1.max(1))
            .unwrap_or(Money::MAX);
        let cap =
            Money::from_cents(revenue_per_batch.mul_bp(INPUT_REVENUE_SHARE_BP).cents() / per_batch);
        orders.push(Order {
            buyer: AccountId::Business(b.id),
            urgency: if current < need_per_day { 0 } else { 1 },
            qty: want,
            max_spend: Some(budget),
            max_unit_price: Some(cap),
        });
    }

    // Households buy food up to a pantry target plus today's meal.
    if good == Good::Food {
        for a in state.agents.values() {
            let want = (PANTRY_TARGET + 1) - a.pantry;
            if want <= 0 {
                continue;
            }
            orders.push(Order {
                buyer: AccountId::Agent(a.id),
                urgency: if a.pantry == 0 { 0 } else { 1 },
                qty: want,
                max_spend: None,
                max_unit_price: None,
            });
        }
    }

    // Stable sort: preserves id order within equal keys. AccountId's derived
    // order puts businesses before agents inside the same urgency tier.
    orders.sort_by_key(|o| (o.urgency, o.buyer));
    orders
}

fn execute_orders(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    good: Good,
    orders: Vec<Order>,
    mut offers: Vec<Offer>,
    acc: &mut DayAccumulator,
) -> Result<(), LedgerError> {
    for order in orders {
        let mut remaining = order.qty;
        let mut budget = order.max_spend;
        for offer in offers.iter_mut() {
            if remaining == 0 {
                break;
            }
            if offer.qty == 0 {
                continue;
            }
            if order.buyer == AccountId::Business(offer.seller) {
                continue; // never trade with yourself
            }
            if let Some(cap) = order.max_unit_price {
                if offer.price > cap {
                    break; // offers are price-sorted: everything further costs more
                }
            }
            let cash = ledger::balance(state, order.buyer)?;
            let spendable = match budget {
                Some(b) => b.min(cash),
                None => cash,
            };
            let affordable = spendable.affordable_units(offer.price);
            let take = remaining.min(offer.qty).min(affordable);
            if take == 0 {
                if affordable == 0 {
                    break; // offers are price-sorted: nothing further is affordable
                }
                continue;
            }
            let Some(cost) = offer.price.checked_mul_qty(take) else {
                break; // unreachable at sane magnitudes; refuse rather than wrap
            };
            ledger::transfer(
                state,
                journal,
                tick,
                order.buyer,
                AccountId::Business(offer.seller),
                cost,
                TxKind::GoodsPurchase {
                    good,
                    qty: take,
                    unit_price: offer.price,
                },
            )?;
            if let Some(seller) = state.businesses.get_mut(&offer.seller) {
                seller.add_stock(good, -take);
                seller.sold_today += take;
                seller.revenue_window += cost;
            }
            match order.buyer {
                AccountId::Agent(id) => {
                    if let Some(agent) = state.agents.get_mut(&id) {
                        agent.pantry += take;
                        agent.total_spent += cost;
                    }
                }
                AccountId::Business(id) => {
                    if let Some(buyer) = state.businesses.get_mut(&id) {
                        buyer.add_stock(good, take);
                        buyer.costs_window += cost;
                    }
                }
            }
            *acc.trade_volume.entry(good).or_insert(0) += take;
            *acc.trade_value.entry(good).or_insert(Money::ZERO) += cost;
            offer.qty -= take;
            remaining -= take;
            budget = budget.map(|b| b - cost);
        }
        if remaining > 0 {
            *acc.unmet_demand.entry(good).or_insert(0) += remaining;
        }
    }
    Ok(())
}

/// Clear all goods markets for this tick, in canonical good order.
pub fn run_goods_markets(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    acc: &mut DayAccumulator,
) -> Result<(), LedgerError> {
    for good in Good::ALL {
        let offers = build_offers(state, good);
        let orders = build_orders(state, good);
        execute_orders(state, journal, tick, good, orders, offers, acc)?;

        // A seller is "stocked out" when it *sold out* — it moved units today,
        // holds nothing, and demand still went unmet. A business with nothing
        // to sell all day (no workers, no production) gets no scarcity signal:
        // it isn't participating in the market, so it must not ratchet prices.
        let unmet = acc.unmet_demand.get(&good).copied().unwrap_or(0);
        if unmet > 0 {
            for b in state.businesses.values_mut() {
                if b.sells == good && b.stock(good) == 0 && b.sold_today > 0 {
                    b.stockout_days += 1;
                }
            }
        }
        let volume = acc.trade_volume.get(&good).copied().unwrap_or(0);
        if volume > 0 {
            let value = acc.trade_value.get(&good).copied().unwrap_or(Money::ZERO);
            let avg = Money::from_cents(value.cents() / volume);
            state.market.last_prices.insert(good, avg);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldConfig;
    use crate::World;

    /// Strip the generated world to a bare stage for controlled market tests.
    fn bare_world() -> World {
        let mut w = World::from_config(WorldConfig::default_with_seed(1));
        for b in w.state.businesses.values_mut() {
            b.inventory.clear();
            b.workers.clear();
        }
        for a in w.state.agents.values_mut() {
            a.pantry = PANTRY_TARGET + 1; // no food demand by default
        }
        w
    }

    fn biz_ids(w: &World) -> Vec<BusinessId> {
        w.state.businesses.keys().copied().collect()
    }

    #[test]
    fn buyer_takes_cheapest_offer_first_with_id_tiebreak() {
        let mut w = bare_world();
        let ids = biz_ids(&w);
        // Both farms sell wheat; make farm[1] cheaper.
        {
            let b0 = w.state.businesses.get_mut(&ids[0]).unwrap();
            b0.sells = Good::Wheat;
            b0.price = Money::from_cents(100);
            b0.add_stock(Good::Wheat, 10);
        }
        {
            let b1 = w.state.businesses.get_mut(&ids[1]).unwrap();
            b1.sells = Good::Wheat;
            b1.price = Money::from_cents(80);
            b1.add_stock(Good::Wheat, 4);
        }
        // The mill (ids[2]) wants wheat: give it a fresh recipe demand.
        {
            let b2 = w.state.businesses.get_mut(&ids[2]).unwrap();
            b2.cash = Money::from_cents(100_000);
        }
        let mut acc = DayAccumulator::default();
        run_goods_markets(&mut w.state, &mut w.journal, 1, &mut acc).unwrap();

        // Mill needed wheat (its recipe consumes wheat) and must have taken
        // the 4 cheap units from farm[1] before touching farm[0].
        let sold_cheap = w.state.businesses[&ids[1]].sold_today;
        let sold_dear = w.state.businesses[&ids[0]].sold_today;
        assert_eq!(sold_cheap, 4, "cheapest offer consumed fully first");
        assert!(sold_dear > 0, "remaining demand moved to next offer");
    }

    #[test]
    fn purchases_respect_cash_limits() {
        let mut w = bare_world();
        let ids = biz_ids(&w);
        {
            let bakery = w.state.businesses.get_mut(&ids[3]).unwrap();
            bakery.add_stock(Good::Food, 100);
            bakery.price = Money::from_cents(300);
        }
        let hungry_id = *w.state.agents.keys().last().unwrap();
        {
            let a = w.state.agents.get_mut(&hungry_id).unwrap();
            a.pantry = 0;
            a.cash = Money::from_cents(650); // affords 2 of 4 wanted units
        }
        w.state.expected_total_money = w.state.total_cash();
        let mut acc = DayAccumulator::default();
        run_goods_markets(&mut w.state, &mut w.journal, 1, &mut acc).unwrap();
        let a = &w.state.agents[&hungry_id];
        assert_eq!(a.pantry, 2);
        assert_eq!(a.cash, Money::from_cents(50));
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    }

    #[test]
    fn unmet_demand_marks_seller_stockout() {
        let mut w = bare_world();
        let ids = biz_ids(&w);
        {
            let bakery = w.state.businesses.get_mut(&ids[3]).unwrap();
            bakery.add_stock(Good::Food, 1); // one unit for a whole town
            bakery.price = Money::from_cents(100);
        }
        for a in w.state.agents.values_mut() {
            a.pantry = 0; // everyone urgent
        }
        let mut acc = DayAccumulator::default();
        run_goods_markets(&mut w.state, &mut w.journal, 1, &mut acc).unwrap();
        assert!(acc.unmet_demand[&Good::Food] > 0);
        assert_eq!(w.state.businesses[&ids[3]].stockout_days, 1);
        // Urgent buyers are ordered by id: the lowest-id hungry agent ate.
        let first = *w.state.agents.keys().next().unwrap();
        assert_eq!(w.state.agents[&first].pantry, 1);
    }
}
