//! The contract kernel (Phase 3, per BRIEF.md §Contracts and negotiation).
//!
//! One type end to end for now: the **recurring supply contract**, in
//! requirements form — the terms lock a unit PRICE (haggled out below the
//! seller's posted price at a logged negotiation table — `negotiation.rs`)
//! and a daily delivery CEILING; each
//! day the buyer takes its current input need up to that ceiling. Price
//! rigid, quantity adaptive. Both rigidities were tried and killed towns
//! (DECISIONS.md #026): weekly delivery lumps starved a hand-to-mouth
//! economy between due dates, and a fixed daily quantity either forced
//! over-buying (buyers bleeding cash for inputs they couldn't use) or
//! anchored the whole chain's sales EMAs at the contracted flow — a
//! stable under-production fixed point no price signal could break.
//! Contracts are hashed state; they bind businesses, not owners, and
//! survive takeovers.
//!
//! Lifecycle: `Active` from signing; each due date either delivers (goods
//! seller→buyer, cash buyer→seller through the ledger) or misses (the
//! failing side pays a cash-capped penalty). Three consecutive misses
//! terminate the contract as `Breached`; an exhausted schedule ends it as
//! `Completed`. Terminal contracts stay in the map as the contract view's
//! archive — they are small, and their history feeds Phase 3's credit
//! scoring.
//!
//! Settlement runs in tick phase 6, after the goods markets. Committed
//! goods are sold goods in waiting: sellers produce toward their rolling
//! per-period commitment, continuously withhold it from market offers, and
//! their glut/stockout signals read free stock only; buyers protect a due
//! payment in their market budget. A producing seller and a solvent buyer
//! settle reliably — a miss means a real shortfall.
//!
//! Negotiation v1 is take-it-or-leave-it posted terms (the buyer decides
//! through the utility engine; the seller's side is a capacity check). The
//! full offer/counteroffer log grows in the next increment
//! (DECISIONS.md #026).

use crate::events::Event;
use crate::goods::{Good, Qty};
use crate::ids::{AccountId, BusinessId, ContractId};
use crate::ledger::{self, LedgerError, TxKind};
use crate::metrics::DayAccumulator;
use crate::money::Money;
use crate::world::{Journal, SimState};
use serde::{Deserialize, Serialize};

/// Ticks between deliveries. Daily — the cadence the rest of the economy
/// already runs on. A weekly cadence was tried first and starved the town:
/// the seller withheld a week's committed stock from the spot market
/// (including from the waiting buyer itself) while everyone lived
/// hand-to-mouth between lumps (DECISIONS.md #026).
pub const CONTRACT_EVERY: u64 = 1;
/// Scheduled deliveries per contract (84 days ≈ one quarter), after which
/// the parties are free to re-sign at fresh prices — expiry is the v1
/// renegotiation channel.
pub const CONTRACT_DELIVERIES: u32 = 84;
/// Penalty for a missed delivery, in basis points of the delivery value,
/// paid by the failing side and capped at what it can afford (banking and
/// enforceable debt arrive with the bank increment).
pub const CONTRACT_PENALTY_BP: i64 = 2500;
/// Consecutive misses that terminate the contract as breached.
pub const BREACH_AFTER_MISSES: u32 = 3;
/// Share of a seller's bare-handed capacity per period that contracts may
/// claim, in basis points. The remainder stays free for the seller's spot
/// customers and for production hiccups — a seller contracted to 100% of
/// capacity misses on any bad week and starves its walk-in demand.
pub const CONTRACT_CAPACITY_SHARE_BP: i64 = 8_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractState {
    Active,
    /// Every scheduled delivery date has passed (delivered or missed).
    Completed,
    /// Terminated after `BREACH_AFTER_MISSES` consecutive misses.
    Breached,
    /// A party walked away voluntarily, paying the exit penalty — the
    /// brief's "breach contract" action. Without this valve, a buyer
    /// locked into spike-priced inputs bleeds out for the whole term: the
    /// reservation-cap refusal that deflates every crisis is contractually
    /// severed, and towns that recovered before contracts died with them
    /// (the seed-7 lesson, DECISIONS.md #026).
    Terminated,
}

/// Which side failed a delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractParty {
    /// The seller could not put up the goods.
    Seller,
    /// The buyer could not pay.
    Buyer,
}

impl ContractParty {
    pub fn label(self) -> &'static str {
        match self {
            ContractParty::Seller => "seller",
            ContractParty::Buyer => "buyer",
        }
    }
}

/// A recurring supply contract. Fields are flat while there is one contract
/// type; a `terms` enum factors out when the second type lands
/// (DECISIONS.md #026).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub id: ContractId,
    pub seller: BusinessId,
    pub buyer: BusinessId,
    pub good: Good,
    /// Daily delivery ceiling: each day the buyer takes its current input
    /// need up to this many units (requirements form).
    pub qty: Qty,
    /// Fixed unit price agreed at signing.
    pub unit_price: Money,
    /// Ticks between deliveries.
    pub every: u64,
    /// Total scheduled delivery days.
    pub deliveries: u32,
    pub start_tick: u64,
    /// Next delivery date. Advances by `every` on every due date, hit or
    /// missed, so the schedule never stalls.
    pub next_due: u64,
    /// Delivery days settled cleanly (including zero-need days — a day
    /// the buyer needed nothing is a satisfied day, not a miss).
    pub delivered: u32,
    pub missed: u32,
    pub consecutive_misses: u32,
    /// Units actually delivered over the contract's life.
    pub delivered_units: Qty,
    /// Cumulative cash actually paid for deliveries — must always equal
    /// `unit_price × delivered_units` (the `contract_reconciliation`
    /// invariant).
    pub paid_total: Money,
    /// Cumulative penalties actually paid (cash-capped, so bounded above
    /// by `penalty events × full ceiling penalty`, not equal to it).
    pub penalties_paid_total: Money,
    pub state: ContractState,
}

impl Contract {
    /// Cash owed for a full-ceiling delivery — the penalty base and the
    /// upper bound of any day's bill.
    pub fn delivery_cost(&self) -> Money {
        self.unit_price
            .checked_mul_qty(self.qty)
            .unwrap_or(Money::MAX)
    }
}

/// Today's take under a contract: the buyer's current daily input need,
/// capped by the contracted ceiling. Zero when the buyer needs nothing.
fn today_take(state: &SimState, c: &Contract) -> Qty {
    state
        .businesses
        .get(&c.buyer)
        .map(|b| crate::market::daily_input_need(b, c.good).min(c.qty))
        .unwrap_or(0)
}

/// Cash a buyer owes for today's takes at `tick` — protected from its
/// market spending that day.
pub fn payment_due_today(state: &SimState, buyer: BusinessId, tick: u64) -> Money {
    state
        .contracts
        .values()
        .filter(|c| c.state == ContractState::Active && c.buyer == buyer && c.next_due == tick)
        .map(|c| {
            c.unit_price
                .checked_mul_qty(today_take(state, c))
                .unwrap_or(Money::MAX)
        })
        .sum()
}

/// Units per day the seller has promised across active contracts for
/// `good`. Counted by the acceptance capacity check (no oversubscription),
/// held out of the seller's market offers, and added to its production
/// target — committed goods are sold goods in waiting.
pub fn committed_per_period(state: &SimState, seller: BusinessId, good: Good) -> Qty {
    state
        .contracts
        .values()
        .filter(|c| c.state == ContractState::Active && c.seller == seller && c.good == good)
        .map(|c| c.qty)
        .sum()
}

/// Stock actually available to the spot market: on hand minus today's
/// contract commitment, floored at zero. Market offers, the glut/stockout
/// price signals and the tool-purchase gate all read this — an earmarked
/// delivery is not overhang.
pub fn free_stock(state: &SimState, seller: BusinessId, good: Good) -> Qty {
    let held = state
        .businesses
        .get(&seller)
        .map(|b| b.stock(good))
        .unwrap_or(0);
    (held - committed_per_period(state, seller, good)).max(0)
}

/// Whether supply contracts may form over this good in v1: durable
/// industrial inputs only. The survival-food chain (wheat, flour) stays
/// spot: its downstream buyers are price-taking households, so no
/// reservation cap disciplines a locked price, and every contract
/// distortion lands in the chain's razor-thin cash margins instead of its
/// prices — ten-year soaks of food-chain contract towns ended in famine
/// every time, while industry-contract towns beat the pre-contract
/// baseline (DECISIONS.md #026). Food-chain contracts return when the
/// bank can float working capital (Phase 3's next increment).
pub fn contractable(good: Good) -> bool {
    !matches!(good, Good::Wheat | Good::Flour | Good::Food)
}

/// Total daily ceiling committed across ALL active contracts for `good`,
/// town-wide — demand that never appears as spot orders. The takeover-
/// revival gate counts it: a good flowing entirely under contract is still
/// a live market, and a revived competitor can win it back at expiry
/// (without this, contract towns never revived their dead farms and ran
/// single-supplier until the supplier stumbled — DECISIONS.md #026).
pub fn town_committed(state: &SimState, good: Good) -> Qty {
    state
        .contracts
        .values()
        .filter(|c| c.state == ContractState::Active && c.good == good)
        .map(|c| c.qty)
        .sum()
}

/// Whether `buyer` already holds an active supply contract for `good`.
pub fn has_active_supply(state: &SimState, buyer: BusinessId, good: Good) -> bool {
    state
        .contracts
        .values()
        .any(|c| c.state == ContractState::Active && c.buyer == buyer && c.good == good)
}

/// The agreed terms of a new supply contract, as handed to [`sign`].
#[derive(Clone, Copy, Debug)]
pub struct SupplyTerms {
    pub seller: BusinessId,
    pub buyer: BusinessId,
    pub good: Good,
    /// Units per delivery.
    pub qty: Qty,
    pub unit_price: Money,
}

/// Register a freshly agreed contract and journal the signing.
pub fn sign(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    terms: SupplyTerms,
) -> ContractId {
    let SupplyTerms {
        seller,
        buyer,
        good,
        qty,
        unit_price,
    } = terms;
    let id = ContractId(state.next_contract_id);
    state.next_contract_id += 1;
    state.contracts.insert(
        id,
        Contract {
            id,
            seller,
            buyer,
            good,
            qty,
            unit_price,
            every: CONTRACT_EVERY,
            deliveries: CONTRACT_DELIVERIES,
            start_tick: tick,
            next_due: tick + CONTRACT_EVERY,
            delivered: 0,
            missed: 0,
            consecutive_misses: 0,
            delivered_units: 0,
            paid_total: Money::ZERO,
            penalties_paid_total: Money::ZERO,
            state: ContractState::Active,
        },
    );
    journal.push_event(
        tick,
        Event::ContractSigned {
            contract: id,
            seller,
            buyer,
            good,
            qty,
            unit_price,
            deliveries: CONTRACT_DELIVERIES,
        },
    );
    id
}

/// A buyer walks away from an underwater contract (decisions phase): pays
/// the seller the exit penalty (`CONTRACT_PENALTY_BP` of one delivery,
/// cash-capped), the contract terminates, and the jilted seller's owner
/// remembers who broke their word.
pub fn buyer_walks_away(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    cid: ContractId,
) -> Result<Money, LedgerError> {
    let Some(c) = state.contracts.get(&cid) else {
        return Ok(Money::ZERO);
    };
    if c.state != ContractState::Active {
        return Ok(Money::ZERO);
    }
    let (seller_id, buyer_id, cost) = (c.seller, c.buyer, c.delivery_cost());
    let seller_owner = state.businesses.get(&seller_id).map(|b| b.owner);
    let buyer_owner = state.businesses.get(&buyer_id).map(|b| b.owner);
    let full = cost.mul_bp(CONTRACT_PENALTY_BP);
    let buyer_cash = ledger::balance(state, AccountId::Business(buyer_id))?;
    let penalty = full.min(buyer_cash).max(Money::ZERO);
    if penalty > Money::ZERO {
        ledger::transfer(
            state,
            journal,
            tick,
            AccountId::Business(buyer_id),
            AccountId::Business(seller_id),
            penalty,
            TxKind::ContractPenalty { contract: cid },
        )?;
        if let Some(b) = state.businesses.get_mut(&buyer_id) {
            b.books.penalties_paid += penalty;
        }
        if let Some(b) = state.businesses.get_mut(&seller_id) {
            b.books.penalties_received += penalty;
        }
    }
    if let Some(c) = state.contracts.get_mut(&cid) {
        c.penalties_paid_total += penalty;
        c.state = ContractState::Terminated;
    }
    journal.push_event(
        tick,
        Event::ContractTerminated {
            contract: cid,
            by: ContractParty::Buyer,
            penalty,
        },
    );
    if let (Some(so), Some(bo)) = (seller_owner, buyer_owner) {
        if so != bo {
            if let Some(a) = state.agents.get_mut(&so) {
                crate::relationships::on_contract_missed(a, bo);
                crate::reputation::on_contract_missed_victim(a, bo);
            }
        }
    }
    Ok(penalty)
}

/// Tick phase 6: settle every active contract due today, in id order.
pub fn settle(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    acc: &mut DayAccumulator,
) -> Result<(), LedgerError> {
    let due: Vec<ContractId> = state
        .contracts
        .iter()
        .filter(|(_, c)| c.state == ContractState::Active && c.next_due <= tick)
        .map(|(id, _)| *id)
        .collect();
    for cid in due {
        let Some(c) = state.contracts.get(&cid) else {
            continue;
        };
        // Requirements form: today's take is the buyer's current need up
        // to the ceiling. A zero-need day settles trivially — the seller
        // owed nothing, so nothing moves and nobody missed.
        let take = today_take(state, c);
        let (seller_id, buyer_id, good, unit_price) = (c.seller, c.buyer, c.good, c.unit_price);
        let cost = unit_price.checked_mul_qty(take).unwrap_or(Money::MAX);
        let seller_stock = state
            .businesses
            .get(&seller_id)
            .map(|b| b.stock(good))
            .unwrap_or(0);
        let buyer_cash = ledger::balance(state, AccountId::Business(buyer_id))?;
        let seller_owner = state.businesses.get(&seller_id).map(|b| b.owner);
        let buyer_owner = state.businesses.get(&buyer_id).map(|b| b.owner);

        if take == 0 {
            if let Some(c) = state.contracts.get_mut(&cid) {
                c.delivered += 1;
                c.consecutive_misses = 0;
                c.next_due += c.every;
            }
        } else if seller_stock >= take && buyer_cash >= cost {
            // --- Delivery: cash through the ledger, goods as a zero-sum
            // trade (conservation targets untouched, like any market trade).
            ledger::transfer(
                state,
                journal,
                tick,
                AccountId::Business(buyer_id),
                AccountId::Business(seller_id),
                cost,
                TxKind::ContractDelivery {
                    contract: cid,
                    good,
                    qty: take,
                    unit_price,
                },
            )?;
            if let Some(seller) = state.businesses.get_mut(&seller_id) {
                seller.add_stock(good, -take);
                seller.sold_today += take;
                seller.revenue_window += cost;
                seller.books.revenue += cost;
            }
            // Contract sales carry the same sales tax as spot trades —
            // contracting must never be a tax dodge (DECISIONS.md #029).
            crate::government::collect_sales_tax(state, journal, tick, seller_id, good, cost)?;
            if let Some(buyer) = state.businesses.get_mut(&buyer_id) {
                buyer.add_stock(good, take);
                buyer.costs_window += cost;
                if good == Good::Tools {
                    buyer.books.tool_costs += cost;
                } else {
                    buyer.books.input_costs += cost;
                }
            }
            if let Some(c) = state.contracts.get_mut(&cid) {
                c.delivered += 1;
                c.consecutive_misses = 0;
                c.delivered_units += take;
                c.paid_total += cost;
                c.next_due += c.every;
            }
            acc.contract_deliveries += 1;
            journal.push_event(
                tick,
                Event::ContractDelivered {
                    contract: cid,
                    good,
                    qty: take,
                    amount: cost,
                },
            );
            // Doing business builds the dyad: each owner finds the other
            // commercially reliable.
            if let (Some(so), Some(bo)) = (seller_owner, buyer_owner) {
                if so != bo {
                    if let Some(a) = state.agents.get_mut(&bo) {
                        crate::relationships::on_contract_delivered(a, so);
                    }
                    if let Some(a) = state.agents.get_mut(&so) {
                        crate::relationships::on_contract_delivered(a, bo);
                    }
                }
            }
        } else {
            // --- Miss: the failing side pays a cash-capped penalty. When
            // both sides failed, the seller answers first (deterministic and
            // documented: goods are promised before payment is).
            let by = if seller_stock < take {
                ContractParty::Seller
            } else {
                ContractParty::Buyer
            };
            let (payer_id, payee_id) = match by {
                ContractParty::Seller => (seller_id, buyer_id),
                ContractParty::Buyer => (buyer_id, seller_id),
            };
            let full = cost.mul_bp(CONTRACT_PENALTY_BP);
            let payer_cash = ledger::balance(state, AccountId::Business(payer_id))?;
            let penalty = full.min(payer_cash).max(Money::ZERO);
            if penalty > Money::ZERO {
                ledger::transfer(
                    state,
                    journal,
                    tick,
                    AccountId::Business(payer_id),
                    AccountId::Business(payee_id),
                    penalty,
                    TxKind::ContractPenalty { contract: cid },
                )?;
                if let Some(b) = state.businesses.get_mut(&payer_id) {
                    b.books.penalties_paid += penalty;
                }
                if let Some(b) = state.businesses.get_mut(&payee_id) {
                    b.books.penalties_received += penalty;
                }
            }
            let mut breached = false;
            if let Some(c) = state.contracts.get_mut(&cid) {
                c.missed += 1;
                c.consecutive_misses += 1;
                c.penalties_paid_total += penalty;
                c.next_due += c.every;
                if c.consecutive_misses >= BREACH_AFTER_MISSES {
                    c.state = ContractState::Breached;
                    breached = true;
                }
            }
            acc.contract_misses += 1;
            journal.push_event(
                tick,
                Event::ContractMissed {
                    contract: cid,
                    by,
                    penalty,
                },
            );
            // The victim's owner learns something private (the dyad sours)
            // and something public (the failer is unreliable — the belief
            // that gossip spreads; contract performance is a reputation
            // driver per BRIEF.md).
            let (failer_owner, victim_owner) = match by {
                ContractParty::Seller => (seller_owner, buyer_owner),
                ContractParty::Buyer => (buyer_owner, seller_owner),
            };
            if let (Some(fo), Some(vo)) = (failer_owner, victim_owner) {
                if fo != vo {
                    if let Some(a) = state.agents.get_mut(&vo) {
                        crate::relationships::on_contract_missed(a, fo);
                        crate::reputation::on_contract_missed_victim(a, fo);
                    }
                }
            }
            if breached {
                journal.push_event(tick, Event::ContractBreached { contract: cid, by });
            }
        }

        // Schedule exhausted (and not already breached): completed.
        let complete = state.contracts.get(&cid).is_some_and(|c| {
            c.state == ContractState::Active && c.delivered + c.missed >= c.deliveries
        });
        if complete {
            if let Some(c) = state.contracts.get_mut(&cid) {
                c.state = ContractState::Completed;
            }
            journal.push_event(tick, Event::ContractCompleted { contract: cid });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldConfig;
    use crate::World;

    /// A controlled stage: seller (farm) supplies wheat to buyer (mill)
    /// daily. Signed at tick 0, first delivery due at tick 1.
    fn stage() -> (World, ContractId, BusinessId, BusinessId) {
        let mut w = World::from_config(WorldConfig::default_with_seed(5));
        let ids: Vec<BusinessId> = w.state.businesses.keys().copied().collect();
        let (farm, mill) = (ids[0], ids[2]);
        for b in w.state.businesses.values_mut() {
            b.inventory.clear();
        }
        // Re-sync conservation targets after stripping the seeded stock.
        for good in Good::ALL {
            let total = w.state.total_goods(good);
            w.state.expected_total_goods.insert(good, total);
        }
        let cid = sign(
            &mut w.state,
            &mut w.journal,
            0,
            SupplyTerms {
                seller: farm,
                buyer: mill,
                good: Good::Wheat,
                qty: 10,
                unit_price: Money::from_cents(500),
            },
        );
        (w, cid, farm, mill)
    }

    fn settle_at(w: &mut World, tick: u64) {
        let mut acc = DayAccumulator::default();
        settle(&mut w.state, &mut w.journal, tick, &mut acc).unwrap();
    }

    #[test]
    fn delivery_moves_goods_and_money_and_advances_the_schedule() {
        let (mut w, cid, farm, mill) = stage();
        // Pin the rate: the exact tax figures below must not move with the
        // world-default calibration.
        w.state.government.sales_tax_bp = 300;
        crate::goods_ledger::produce(&mut w.state, farm, Good::Wheat, 15);
        let mill_cash = w.state.businesses[&mill].cash;
        let farm_cash = w.state.businesses[&farm].cash;
        settle_at(&mut w, 1);
        let c = &w.state.contracts[&cid];
        assert_eq!(c.delivered, 1);
        assert_eq!(c.next_due, 2);
        assert_eq!(
            c.delivered_units, 10,
            "take = min(ceiling 10, mill need 12)"
        );
        assert_eq!(c.paid_total, Money::from_cents(5_000));
        assert_eq!(c.state, ContractState::Active);
        assert_eq!(w.state.businesses[&farm].stock(Good::Wheat), 5);
        assert_eq!(w.state.businesses[&mill].stock(Good::Wheat), 10);
        assert_eq!(
            w.state.businesses[&mill].cash,
            mill_cash - Money::from_cents(5_000)
        );
        // The farm nets the gross minus the 3% sales tax ($1.50), remitted
        // at the same settlement site (Phase 4).
        assert_eq!(
            w.state.businesses[&farm].cash,
            farm_cash + Money::from_cents(5_000 - 150)
        );
        assert_eq!(
            w.state.businesses[&farm].books.taxes_paid,
            Money::from_cents(150)
        );
        assert_eq!(w.state.government.cash, Money::from_cents(150));
        assert_eq!(w.state.businesses[&farm].sold_today, 10);
        assert_eq!(
            w.state.businesses[&farm].books.revenue,
            Money::from_cents(5_000)
        );
        assert_eq!(
            w.state.businesses[&mill].books.input_costs,
            Money::from_cents(5_000)
        );
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
        assert!(w.journal.events.iter().any(
            |e| matches!(e.event, Event::ContractDelivered { contract, .. } if contract == cid)
        ));
    }

    #[test]
    fn seller_shortfall_is_a_penalized_miss_and_delivery_resets_the_run() {
        let (mut w, cid, farm, mill) = stage();
        // Nothing on hand at tick 1: seller miss. Penalty = 25% of $50 = $12.50.
        let farm_cash = w.state.businesses[&farm].cash;
        settle_at(&mut w, 1);
        let c = &w.state.contracts[&cid];
        assert_eq!((c.delivered, c.missed, c.consecutive_misses), (0, 1, 1));
        assert_eq!(c.penalties_paid_total, Money::from_cents(1_250));
        assert_eq!(
            w.state.businesses[&farm].cash,
            farm_cash - Money::from_cents(1_250)
        );
        assert_eq!(
            w.state.businesses[&farm].books.penalties_paid,
            Money::from_cents(1_250)
        );
        assert_eq!(
            w.state.businesses[&mill].books.penalties_received,
            Money::from_cents(1_250)
        );
        assert!(w.journal.events.iter().any(|e| matches!(
            e.event,
            Event::ContractMissed { contract, by: ContractParty::Seller, .. } if contract == cid
        )));
        // Stock arrives; the next due date delivers and the run resets.
        crate::goods_ledger::produce(&mut w.state, farm, Good::Wheat, 10);
        settle_at(&mut w, 2);
        let c = &w.state.contracts[&cid];
        assert_eq!((c.delivered, c.missed, c.consecutive_misses), (1, 1, 0));
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn three_consecutive_misses_breach_and_terminate() {
        let (mut w, cid, farm, _mill) = stage();
        settle_at(&mut w, 1);
        settle_at(&mut w, 2);
        settle_at(&mut w, 3);
        let c = &w.state.contracts[&cid];
        assert_eq!(c.state, ContractState::Breached);
        assert_eq!(c.consecutive_misses, BREACH_AFTER_MISSES);
        assert!(w.journal.events.iter().any(
            |e| matches!(e.event, Event::ContractBreached { contract, .. } if contract == cid)
        ));
        // Terminal: later due dates settle nothing further.
        let missed_before = c.missed;
        crate::goods_ledger::produce(&mut w.state, farm, Good::Wheat, 50);
        settle_at(&mut w, 4);
        assert_eq!(w.state.contracts[&cid].missed, missed_before);
        assert_eq!(w.state.contracts[&cid].delivered, 0);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn exhausted_schedule_completes() {
        let (mut w, cid, farm, mill) = stage();
        crate::goods_ledger::produce(
            &mut w.state,
            farm,
            Good::Wheat,
            10 * CONTRACT_DELIVERIES as Qty,
        );
        // Fund the buyer for the whole schedule (84 × $50).
        {
            let mill_b = w.state.businesses.get_mut(&mill).unwrap();
            mill_b.cash = Money::from_cents(500_000);
            mill_b.books = crate::business::Books::new(Money::from_cents(500_000));
        }
        w.state.expected_total_money = w.state.total_cash();
        for k in 1..=u64::from(CONTRACT_DELIVERIES) {
            settle_at(&mut w, k);
        }
        let c = &w.state.contracts[&cid];
        assert_eq!(c.state, ContractState::Completed);
        assert_eq!(c.delivered, CONTRACT_DELIVERIES);
        assert_eq!(c.delivered_units, 10 * CONTRACT_DELIVERIES as Qty);
        assert_eq!(
            c.paid_total,
            Money::from_cents(5_000)
                .checked_mul_qty(i64::from(CONTRACT_DELIVERIES))
                .unwrap()
        );
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(e.event, Event::ContractCompleted { contract } if contract == cid)));
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn broke_buyer_misses_and_pays_what_it_can() {
        let (mut w, cid, farm, mill) = stage();
        crate::goods_ledger::produce(&mut w.state, farm, Good::Wheat, 10);
        // The mill can afford neither the $50 delivery nor the full $12.50
        // penalty: it pays its whole $9 and no more.
        {
            let mill_b = w.state.businesses.get_mut(&mill).unwrap();
            mill_b.cash = Money::from_cents(900);
            mill_b.books = crate::business::Books::new(Money::from_cents(900));
        }
        w.state.expected_total_money = w.state.total_cash();
        settle_at(&mut w, 1);
        let c = &w.state.contracts[&cid];
        assert_eq!((c.delivered, c.missed), (0, 1));
        assert_eq!(c.penalties_paid_total, Money::from_cents(900));
        assert_eq!(w.state.businesses[&mill].cash, Money::ZERO);
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(
                e.event,
                Event::ContractMissed { contract, by: ContractParty::Buyer, penalty } if contract == cid && penalty == Money::from_cents(900)
            )));
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn a_zero_need_day_settles_trivially() {
        // A contract for a good the buyer's recipe never consumes: the
        // take is zero, so the day is satisfied with nothing owed and no
        // miss — the requirements form never forces goods on a buyer.
        // (Fresh world, no other contracts due.)
        let mut w = World::from_config(WorldConfig::default_with_seed(5));
        let ids: Vec<BusinessId> = w.state.businesses.keys().copied().collect();
        let (farm, mill) = (ids[0], ids[2]);
        let cid = sign(
            &mut w.state,
            &mut w.journal,
            0,
            SupplyTerms {
                seller: farm,
                buyer: mill,
                good: Good::Food,
                qty: 5,
                unit_price: Money::from_cents(100),
            },
        );
        crate::goods_ledger::produce(&mut w.state, farm, Good::Food, 20);
        let farm_cash = w.state.businesses[&farm].cash;
        let mill_cash = w.state.businesses[&mill].cash;
        settle_at(&mut w, 1);
        let c = &w.state.contracts[&cid];
        assert_eq!((c.delivered, c.missed, c.delivered_units), (1, 0, 0));
        assert_eq!(c.next_due, 2);
        assert_eq!(w.state.businesses[&farm].cash, farm_cash);
        assert_eq!(w.state.businesses[&mill].cash, mill_cash);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn misses_sour_the_dyad_and_seed_unreliable_beliefs() {
        let (mut w, _cid, farm, mill) = stage();
        let farm_owner = w.state.businesses[&farm].owner;
        let mill_owner = w.state.businesses[&mill].owner;
        settle_at(&mut w, 1);
        let victim = &w.state.agents[&mill_owner];
        let rel = crate::relationships::relation_toward(victim, farm_owner);
        assert!(rel.commercial_reliability < crate::relationships::NEUTRAL);
        let belief = crate::reputation::belief_about(victim, farm_owner);
        assert!(belief.reliable < crate::reputation::NEUTRAL);
        // A clean delivery builds the dyad back.
        crate::goods_ledger::produce(&mut w.state, farm, Good::Wheat, 10);
        settle_at(&mut w, 2);
        let after = crate::relationships::relation_toward(&w.state.agents[&mill_owner], farm_owner);
        assert!(after.commercial_reliability > rel.commercial_reliability);
    }

    #[test]
    fn query_helpers_see_only_active_contracts() {
        let (mut w, cid, farm, mill) = stage();
        assert_eq!(
            payment_due_today(&w.state, mill, 1),
            Money::from_cents(5_000)
        );
        assert_eq!(payment_due_today(&w.state, mill, 2), Money::ZERO);
        assert_eq!(committed_per_period(&w.state, farm, Good::Wheat), 10);
        assert_eq!(committed_per_period(&w.state, farm, Good::Flour), 0);
        // Free stock nets the rolling commitment out, floored at zero.
        assert_eq!(free_stock(&w.state, farm, Good::Wheat), 0);
        crate::goods_ledger::produce(&mut w.state, farm, Good::Wheat, 25);
        assert_eq!(free_stock(&w.state, farm, Good::Wheat), 15);
        assert!(has_active_supply(&w.state, mill, Good::Wheat));
        assert!(!has_active_supply(&w.state, mill, Good::Flour));
        w.state.contracts.get_mut(&cid).unwrap().state = ContractState::Breached;
        assert_eq!(payment_due_today(&w.state, mill, 1), Money::ZERO);
        assert_eq!(committed_per_period(&w.state, farm, Good::Wheat), 0);
        assert_eq!(free_stock(&w.state, farm, Good::Wheat), 25);
        assert!(!has_active_supply(&w.state, mill, Good::Wheat));
    }
}
