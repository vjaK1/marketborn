//! Decisions phase: business owners.
//!
//! Daily: owner capital injections and emergency downsizing. Every seven
//! days, staggered by business id: price review (utility-scored through
//! the Phase 2 decision engine — the owner's traits weight the choice and
//! the record is journaled), wage review, dividend. Remaining reviews move
//! onto the engine incrementally (AGENT_DESIGN.md).

use crate::agent::Traits;
use crate::decision::{self, PriceAction};
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
/// Strictly above the full production buffer (OUTPUT_TARGET_DAYS + 1 = 5
/// days), so a producer at its normal buffer never reads as glutted
/// (DECISIONS.md #015). Read by the goods market's tool-purchase gate (no
/// capital spending while glutted); the price review's glut/stockout/idle
/// thresholds now live in the decision engine's scoring
/// (`decision::score_price_action`).
pub const GLUT_LIGHT_DAYS: Qty = 6;

struct ReviewPlan {
    window_profit: Money,
    dry_windows: u32,
    new_price: Option<(Money, Money)>,
    new_wage: Option<(Money, Money)>,
    dividend: Option<(AgentId, Money)>,
}

/// Weekly takeover reviews (DECISIONS.md #021): wealthy, entrepreneurial
/// non-owners buy moribund businesses (no staff, and even the sitting
/// owner's savings cannot fund one hire) from their broke owners at asset
/// value, then restart them through the ordinary injection/hiring
/// machinery — entry/exit's first slice, turning dead firms into
/// opportunities instead of absorbing states. The seller becomes a job
/// seeker; the buyer leaves any wage job to run the firm.
fn takeovers(state: &mut SimState, journal: &mut Journal, tick: u64) -> Result<(), LedgerError> {
    let agent_ids: Vec<AgentId> = state.agents.keys().copied().collect();
    for aid in agent_ids {
        if !(tick + u64::from(aid.0)).is_multiple_of(REVIEW_PERIOD) {
            continue;
        }
        let deal: Option<(BusinessId, AgentId, Money)> = {
            let Some(a) = state.agents.get(&aid) else {
                continue;
            };
            if a.owns.is_some()
                || !decision::takeover_appetite(a.traits.ambition, a.traits.risk_tolerance)
            {
                continue;
            }
            // Best moribund target by asset value; id order keeps ties on
            // the lower id.
            let mut best: Option<(BusinessId, AgentId, Money)> = None;
            for b in state.businesses.values() {
                if !b.workers.is_empty() {
                    continue;
                }
                let hire_floor = b
                    .wage
                    .checked_mul_qty(crate::systems::labor::HIRING_CASH_DAYS)
                    .unwrap_or(Money::MAX);
                if b.cash >= hire_floor {
                    continue;
                }
                // The sitting owner has first refusal daily via the
                // injection rule; moribund means even their savings can't
                // fund a hire.
                let owner_avail = state
                    .agents
                    .get(&b.owner)
                    .map(|o| (o.cash - OWNER_RESERVE).max(Money::ZERO))
                    .unwrap_or(Money::ZERO);
                if b.cash + owner_avail >= hire_floor {
                    continue;
                }
                // Only buy where the market wants the product: standing
                // demand for the good must exist right now. Without this,
                // serial zombie entrepreneurship (buying firms nobody buys
                // from) collapsed whole towns. A stricter shortage gate
                // (demand > offered) was tried and rejected: a dead
                // business's own leftover stock masks the coming shortage
                // and blocks the exact revival that restores competition
                // (DECISIONS.md #021). Contract-committed flow counts as
                // demand — a good bought entirely under contract is a live
                // market the revived firm can compete for (#026).
                if crate::market::depth(state, b.sells, tick).demand_qty == 0
                    && crate::contracts::town_committed(state, b.sells) == 0
                {
                    continue;
                }
                // Equity = assets (cash on hand + inventory at market).
                let price = b.cash + b.inventory_value(&state.market.last_prices);
                // The buyer must afford the price, two hires of restart
                // capital, and keep a personal reserve.
                let restart = hire_floor.checked_mul_qty(2).unwrap_or(Money::MAX);
                if a.cash < price + restart + OWNER_RESERVE {
                    continue;
                }
                let better = match &best {
                    None => true,
                    Some((_, _, p0)) => price > *p0,
                };
                if better {
                    best = Some((b.id, b.owner, price));
                }
            }
            best
        };
        let Some((bid, seller, price)) = deal else {
            continue;
        };
        ledger::transfer(
            state,
            journal,
            tick,
            AccountId::Agent(aid),
            AccountId::Agent(seller),
            price,
            TxKind::BusinessSale,
        )?;
        let old_employer = state.agents.get(&aid).and_then(|a| a.employer);
        if let Some(eb) = old_employer {
            if let Some(b) = state.businesses.get_mut(&eb) {
                b.workers.retain(|w| *w != aid);
            }
        }
        if let Some(a) = state.agents.get_mut(&aid) {
            a.owns = Some(bid);
            a.employer = None;
            crate::relationships::on_deal(a, seller);
        }
        if let Some(o) = state.agents.get_mut(&seller) {
            o.owns = None;
            crate::relationships::on_deal(o, aid);
        }
        if let Some(b) = state.businesses.get_mut(&bid) {
            b.owner = aid;
        }
        let capital_after = state
            .agents
            .get(&aid)
            .map(|a| a.cash)
            .unwrap_or(Money::ZERO);
        journal.push_decision(decision::DecisionRecord {
            seq: 0,
            tick,
            actor: aid,
            detail: decision::DecisionDetail::Takeover {
                business: bid,
                seller,
                price,
                capital_after,
            },
        });
        journal.push_event(
            tick,
            Event::BusinessSold {
                business: bid,
                from: seller,
                to: aid,
                price,
            },
        );
    }
    Ok(())
}

/// Weekly supply-contract formation (Phase 3, DECISIONS.md #026), on the
/// buyer business's review stagger. The buyer's owner weighs locking each
/// recipe input's daily delivery — at the cheapest capable seller's posted
/// price minus the commitment discount — against staying on the spot
/// market, through the utility engine. Negotiation v1 is take-it-or-leave-
/// it: the seller's side is a capacity check (posted terms are their own
/// offer); the offer/counteroffer log is the next increment. Runs before
/// the owner reviews, so terms read the prices the week actually traded at.
fn supply_contract_reviews(state: &mut SimState, journal: &mut Journal, tick: u64) {
    let buyer_ids: Vec<BusinessId> = state.businesses.keys().copied().collect();
    for bid in buyer_ids {
        if !(tick + u64::from(bid.0)).is_multiple_of(REVIEW_PERIOD) {
            continue;
        }
        // First, review existing commitments: a contract whose locked
        // price has run past what the input can earn back (the reservation
        // cap) is a slow bleed the buyer must be able to stop — without
        // this valve, buyers locked in at crisis prices die with their
        // towns instead of deflating them (DECISIONS.md #026). Honesty
        // widens how far underwater an owner goes before walking: 0–10%
        // past the cap.
        let underwater: Vec<(
            crate::ids::ContractId,
            crate::goods::Good,
            Money,
            Money,
            i64,
        )> = {
            let Some(b) = state.businesses.get(&bid) else {
                continue;
            };
            let honesty = state
                .agents
                .get(&b.owner)
                .map(|a| a.traits.honesty)
                .unwrap_or(50);
            let tolerance_bp = i64::from(honesty) * 10;
            state
                .contracts
                .values()
                .filter(|c| c.state == crate::contracts::ContractState::Active && c.buyer == bid)
                .filter_map(|c| {
                    let cap = market::input_reservation_cap(b, c.good);
                    let ceiling = cap + cap.mul_bp(tolerance_bp);
                    (c.unit_price > ceiling).then_some((
                        c.id,
                        c.good,
                        c.unit_price,
                        cap,
                        tolerance_bp,
                    ))
                })
                .collect()
        };
        for (cid, good, unit_price, cap, tolerance_bp) in underwater {
            let owner = state.businesses.get(&bid).map(|b| b.owner);
            match crate::contracts::buyer_walks_away(state, journal, tick, cid) {
                Ok(penalty) => {
                    if let Some(actor) = owner {
                        journal.push_decision(decision::DecisionRecord {
                            seq: 0,
                            tick,
                            actor,
                            detail: decision::DecisionDetail::ContractExit {
                                business: bid,
                                contract: cid,
                                good,
                                unit_price,
                                cap,
                                tolerance_bp,
                                penalty,
                            },
                        });
                    }
                }
                Err(_) => continue,
            }
        }
        let input_goods: Vec<crate::goods::Good> = state
            .businesses
            .get(&bid)
            .map(|b| b.recipe.inputs.iter().map(|(g, _)| *g).collect())
            .unwrap_or_default();
        for good in input_goods {
            if !crate::contracts::contractable(good) {
                continue;
            }
            /// Everything the table needs, gathered under one immutable
            /// borrow before the haggle mutates the journal.
            struct TablePlan {
                seller: BusinessId,
                seller_greed: u8,
                posted: Money,
                qty: Qty,
                cap: Money,
                cover_days: Qty,
                buyer_owner: AgentId,
                buyer_greed: u8,
                buyer_risk_tolerance: u8,
            }
            let plan: Option<TablePlan> = {
                let Some(b) = state.businesses.get(&bid) else {
                    continue;
                };
                // A firm with nobody working signs nothing, and one
                // contract per input good at a time.
                if b.workers.is_empty() || crate::contracts::has_active_supply(state, bid, good) {
                    continue;
                }
                let need = market::daily_input_need(b, good);
                if need == 0 {
                    continue;
                }
                let want = need * crate::contracts::CONTRACT_EVERY as Qty;
                let cap = market::input_reservation_cap(b, good);
                let Some(owner) = state.agents.get(&b.owner) else {
                    continue;
                };
                // The buyer sits down with ONE seller a week: the cheapest
                // capable one it will deal with. If that table ends in
                // impasse, next week's review tries again at whatever
                // prices then hold.
                let mut sellers: Vec<&crate::business::Business> = state
                    .businesses
                    .values()
                    .filter(|s| s.id != bid && s.sells == good && !s.workers.is_empty())
                    .collect();
                sellers.sort_by_key(|s| (s.price, s.id));
                let mut found = None;
                for s in sellers {
                    // Partial coverage: the delivery is capped at the
                    // contractable share of the seller's bare-handed
                    // capacity per period, net of what it has already
                    // promised — no oversubscription, and the seller keeps
                    // headroom for its spot customers and for bad weeks.
                    // The buyer's spot orders top up the rest; multi-
                    // sourcing one good across sellers is a later
                    // increment.
                    let capacity_per_period = s.workers.len() as Qty
                        * s.recipe.batches_per_worker
                        * s.recipe.output.1.max(1)
                        * crate::contracts::CONTRACT_EVERY as Qty;
                    // Floored at one unit: the share cap must not round a
                    // single-worker stage (mine, steelworks) out of the
                    // contract economy — committing fully to your only
                    // customer is sound under requirements form.
                    let contractable = (capacity_per_period
                        * crate::contracts::CONTRACT_CAPACITY_SHARE_BP as Qty
                        / 10_000)
                        .max((capacity_per_period > 0) as Qty);
                    let committed = crate::contracts::committed_per_period(state, s.id, good);
                    let qty = want.min(contractable - committed);
                    if qty < 1 {
                        continue;
                    }
                    // No deals with the publicly unreliable (same floor as
                    // hiring refusal, DECISIONS.md #025).
                    if crate::reputation::belief_about(owner, s.owner).reliable
                        < crate::reputation::RELIABLE_HIRING_FLOOR
                    {
                        continue;
                    }
                    // The buyer must afford one day's take at the posted
                    // price (the table can only improve on it).
                    let cost = s.price.checked_mul_qty(qty).unwrap_or(Money::MAX);
                    if market::market_budget(state, b, tick) < cost {
                        continue;
                    }
                    let seller_greed = state
                        .agents
                        .get(&s.owner)
                        .map(|a| a.traits.greed)
                        .unwrap_or(50);
                    found = Some(TablePlan {
                        seller: s.id,
                        seller_greed,
                        posted: s.price,
                        qty,
                        cap,
                        cover_days: b.stock(good) / need,
                        buyer_owner: b.owner,
                        buyer_greed: owner.traits.greed,
                        buyer_risk_tolerance: owner.traits.risk_tolerance,
                    });
                    break;
                }
                found
            };
            let Some(p) = plan else {
                continue;
            };
            // The table: three bounded rounds, every move logged with its
            // reason (BRIEF: "log every negotiation completely").
            let table = crate::negotiation::haggle(p.posted, p.cap, p.buyer_greed, p.seller_greed);
            let Some(agreed) = table.agreed else {
                journal.push_negotiation(crate::negotiation::NegotiationRecord {
                    seq: 0,
                    tick,
                    buyer: bid,
                    seller: p.seller,
                    good,
                    qty: p.qty,
                    rounds: table.rounds,
                    outcome: crate::negotiation::NegotiationOutcome::Impasse,
                });
                continue;
            };
            // The achieved discount — not a flat constant — is what the
            // buyer's Sign/StaySpot review weighs: a stingy seller can win
            // the table and still lose the deal.
            let inputs = crate::decision::SupplyContractInputs {
                discount_bp: crate::negotiation::achieved_discount_bp(p.posted, agreed),
                cover_days: p.cover_days,
                greed: p.buyer_greed,
                risk_tolerance: p.buyer_risk_tolerance,
            };
            let (chosen, considered) = crate::decision::choose_contract_action(&inputs);
            journal.push_decision(decision::DecisionRecord {
                seq: 0,
                tick,
                actor: p.buyer_owner,
                detail: decision::DecisionDetail::SupplyReview {
                    business: bid,
                    seller: p.seller,
                    good,
                    qty: p.qty,
                    unit_price: agreed,
                    inputs,
                    considered,
                    chosen,
                },
            });
            let outcome = if chosen == crate::decision::ContractAction::Sign {
                let cid = crate::contracts::sign(
                    state,
                    journal,
                    tick,
                    crate::contracts::SupplyTerms {
                        seller: p.seller,
                        buyer: bid,
                        good,
                        qty: p.qty,
                        unit_price: agreed,
                    },
                );
                crate::negotiation::NegotiationOutcome::Signed { contract: cid }
            } else {
                crate::negotiation::NegotiationOutcome::BuyerDeclined { unit_price: agreed }
            };
            journal.push_negotiation(crate::negotiation::NegotiationRecord {
                seq: 0,
                tick,
                buyer: bid,
                seller: p.seller,
                good,
                qty: p.qty,
                rounds: table.rounds,
                outcome,
            });
        }
    }
}

/// Cost of one day of recipe inputs at last observed market prices —
/// shared by the dividend buffer and the working-capital injection.
fn daily_input_cost(state: &SimState, b: &crate::business::Business) -> Money {
    let mut total = Money::ZERO;
    for (good, _) in &b.recipe.inputs {
        let need = market::daily_input_need(b, *good);
        let price = state
            .market
            .last_prices
            .get(good)
            .copied()
            .unwrap_or(Money::ZERO);
        total += price.checked_mul_qty(need).unwrap_or(Money::ZERO);
    }
    total
}

pub fn run(state: &mut SimState, journal: &mut Journal, tick: u64) -> Result<(), LedgerError> {
    // Takeovers run first, so a freshly bought business gets its new
    // owner's capital injection in this same pass; contract formation
    // follows, before the owner reviews reprice anything.
    takeovers(state, journal, tick)?;
    supply_contract_reviews(state, journal, tick);
    let business_ids: Vec<BusinessId> = state.businesses.keys().copied().collect();
    for bid in business_ids {
        // --- Daily: owner capital injection — the brief's "invest" action.
        // Two triggers: a business too broke to fund a single hire, and a
        // STAFFED business whose recipe needs inputs it has no budget for
        // after the payroll reserve. The second is load-bearing: without
        // it, a firm hovering just above the hire floor pays wages forever
        // while producing nothing (no input budget), its rich owner never
        // steps in, and its silent input demand keeps every upstream
        // supplier failing the revival gate — the staffed-zombie deadlock
        // that froze whole towns (DECISIONS.md #026). ---
        let inject: Option<(AgentId, Money)> = {
            let Some(b) = state.businesses.get(&bid) else {
                continue;
            };
            let hire_floor = b
                .wage
                .checked_mul_qty(crate::systems::labor::HIRING_CASH_DAYS)
                .unwrap_or(Money::MAX);
            let input_day = daily_input_cost(state, b);
            let hiring_gap = b.cash < hire_floor;
            let input_blocked = !b.workers.is_empty()
                && input_day > Money::ZERO
                && market::market_budget(state, b, tick) < input_day;
            if !hiring_gap && !input_blocked {
                None
            } else {
                let owner_cash = state
                    .agents
                    .get(&b.owner)
                    .map(|a| a.cash)
                    .unwrap_or(Money::ZERO);
                let available = owner_cash - OWNER_RESERVE;
                // Fund two hires of runway plus a week of inputs.
                let input_week = input_day.checked_mul_qty(7).unwrap_or(Money::MAX);
                let want =
                    hire_floor.checked_mul_qty(2).unwrap_or(Money::MAX) + input_week - b.cash;
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
            if let Some(b) = state.businesses.get_mut(&bid) {
                b.books.owner_invested += amount;
            }
            journal.push_event(
                tick,
                Event::OwnerInvested {
                    business: bid,
                    owner,
                    amount,
                },
            );
        }

        // --- Daily: distressed borrowing (Phase 3, DECISIONS.md #027).
        // Own money first (the injection above); if the business is STILL
        // short of a hire or a day of inputs, the owner weighs bank credit
        // through the utility engine. Runway sets the urgency, the rate is
        // the price, and debt aversion (low risk tolerance) weighs it —
        // the probe_rate_shock transmission channel. Borrows always
        // journal; declines and refusals journal on the weekly stagger to
        // bound the record volume. ---
        let borrow: Option<(Money, decision::BorrowInputs)> = {
            let Some(b) = state.businesses.get(&bid) else {
                continue;
            };
            let hire_floor = b
                .wage
                .checked_mul_qty(crate::systems::labor::HIRING_CASH_DAYS)
                .unwrap_or(Money::MAX);
            let input_day = daily_input_cost(state, b);
            let hiring_gap = b.cash < hire_floor;
            let input_blocked = !b.workers.is_empty()
                && input_day > Money::ZERO
                && market::market_budget(state, b, tick) < input_day;
            if !hiring_gap && !input_blocked {
                None
            } else {
                let input_week = input_day.checked_mul_qty(7).unwrap_or(Money::MAX);
                let want =
                    hire_floor.checked_mul_qty(2).unwrap_or(Money::MAX) + input_week - b.cash;
                if want < crate::bank::MIN_LOAN {
                    None
                } else {
                    let payroll = b.daily_payroll();
                    // Runway in days; businesses with no payroll have no
                    // urgency clock — the bank does not do venture lending
                    // for restarts (owner injection and takeover cover
                    // those), so a neutral runway leaves the rate to
                    // decide.
                    let days_left = if payroll > Money::ZERO {
                        (b.cash.cents() / payroll.cents()).min(9)
                    } else {
                        5
                    };
                    state.agents.get(&b.owner).map(|owner| {
                        (
                            want,
                            decision::BorrowInputs {
                                payroll_days_left: days_left,
                                rate_bp: state.bank.base_rate_bp,
                                risk_tolerance: owner.traits.risk_tolerance,
                            },
                        )
                    })
                }
            }
        };
        if let Some((amount, inputs)) = borrow {
            let (chosen, considered) = decision::choose_borrow_action(&inputs);
            let stagger_day = (tick + u64::from(bid.0)).is_multiple_of(REVIEW_PERIOD);
            let owner = state.businesses.get(&bid).map(|b| b.owner);
            let record = |journal: &mut Journal, refused: Option<crate::bank::CreditRefusal>| {
                if let Some(actor) = owner {
                    journal.push_decision(decision::DecisionRecord {
                        seq: 0,
                        tick,
                        actor,
                        detail: decision::DecisionDetail::BorrowReview {
                            business: bid,
                            amount,
                            inputs,
                            considered: considered.clone(),
                            chosen,
                            refused,
                        },
                    });
                }
            };
            if chosen == decision::BorrowAction::Borrow {
                match crate::bank::assess(state, bid, amount) {
                    Ok(()) => {
                        crate::bank::issue(state, journal, tick, bid, amount)?;
                        record(journal, None);
                    }
                    Err(refusal) => {
                        if stagger_day {
                            record(journal, Some(refusal));
                        }
                    }
                }
            } else if stagger_day {
                record(journal, None);
            }
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
            let firing_owner = state.businesses.get(&bid).map(|b| b.owner);
            if let Some(b) = state.businesses.get_mut(&bid) {
                b.workers.pop();
            }
            if let Some(a) = state.agents.get_mut(&aid) {
                a.employer = None;
                crate::memory::remember(a, crate::memory::MemoryKind::FiredBy(bid), tick, 70);
                if let Some(o) = firing_owner {
                    crate::relationships::on_fired(a, o);
                    crate::reputation::on_fired_victim(a, o);
                }
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
            // The glut signal reads free stock: the rolling buffer earmarked
            // for contract deliveries is sold stock in waiting, not overhang
            // to be priced away.
            let stock = crate::contracts::free_stock(state, bid, b.sells);
            let min_step = Money::from_cents(1);

            let window_profit = b.revenue_window - b.costs_window;
            // Phase 2: the price review is utility-scored. The owner's
            // traits weight the choice; the scored record is journaled for
            // the inspector. Capacity is bare-handed — tools are optional
            // overdrive, and counting their bonus would make every
            // equipped business read its upgrade as idle capacity.
            let owner_traits = state
                .agents
                .get(&b.owner)
                .map(|a| a.traits)
                .unwrap_or(Traits::NEUTRAL);
            // Shocks scale the possible here exactly as in production:
            // a drought-throttled farm is not idle (Phase 4).
            let capacity_units = b.workers.len() as Qty
                * b.recipe.batches_per_worker
                * b.recipe.output.1.max(1)
                * crate::shocks::capacity_bp(state, b.kind)
                / 10_000;
            // A zero-revenue window extends the dry run; any sale resets it.
            let dry_windows = if b.revenue_window == Money::ZERO {
                b.dry_windows + 1
            } else {
                0
            };
            let inputs = decision::price_inputs(
                b.stockout_days,
                stock,
                ema_day,
                capacity_units,
                !window_profit.is_negative(),
                dry_windows,
                owner_traits,
            );
            let (action, considered) = decision::choose_price_action(&inputs);
            journal.push_decision(decision::DecisionRecord {
                seq: 0, // assigned by the journal
                tick,
                actor: b.owner,
                detail: decision::DecisionDetail::PriceReview {
                    business: bid,
                    inputs,
                    considered,
                    chosen: action,
                },
            });
            let mut new_price = None;
            let repriced = match action {
                PriceAction::Raise => Some(
                    (b.price + b.price.mul_bp(PRICE_RAISE_BP).max(min_step)).min(PRICE_CEILING),
                ),
                PriceAction::CutHeavy => Some(
                    (b.price - b.price.mul_bp(PRICE_CUT_HEAVY_BP).max(min_step)).max(PRICE_FLOOR),
                ),
                PriceAction::CutLight => Some(
                    (b.price - b.price.mul_bp(PRICE_CUT_LIGHT_BP).max(min_step)).max(PRICE_FLOOR),
                ),
                PriceAction::Hold => None,
            };
            if let Some(p) = repriced {
                if p != b.price {
                    new_price = Some((b.price, p));
                }
            }

            let mut new_wage = None;
            // Raise wages to attract labor only from a position of strength:
            // STRICTLY positive window profit. `>= 0` was a latent death
            // trap — a stone-dead business has revenue 0 − costs 0 = 0,
            // "non-negative", and ratcheted +5% weekly forever until its
            // posted wage hit the ceiling, which then priced both direct
            // rehiring (the hiring cash gate) and takeover revival (restart
            // capital = hires at that wage) out of existence for the whole
            // town (DECISIONS.md #026).
            let hire_floor = b
                .wage
                .checked_mul_qty(crate::systems::labor::HIRING_CASH_DAYS)
                .unwrap_or(Money::MAX);
            if b.vacancies() > 0
                && b.vacancy_days >= REVIEW_PERIOD as u32
                && window_profit > Money::ZERO
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
            } else if b.vacancies() > 0 && b.cash < hire_floor && b.wage > WAGE_FLOOR {
                // An offer the till cannot fund is not an offer: walk the
                // posted wage down toward what the business can actually
                // pay, so a dead firm becomes hirable-into (or cheaply
                // buyable) again instead of fossilizing at a fantasy wage.
                let cut = (b.wage - b.wage.mul_bp(WAGE_CUT_BP).max(min_step)).max(WAGE_FLOOR);
                if cut != b.wage {
                    new_wage = Some((b.wage, cut));
                }
            }

            // Dividend: pay out a quarter of cash above a survival buffer
            // (three weeks of payroll plus a week of input purchases at last
            // observed prices). The rounding remainder — and the other 75% —
            // stays with the business.
            let input_week = daily_input_cost(state, b)
                .checked_mul_qty(7)
                .unwrap_or(Money::MAX);
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
                dry_windows,
                new_price,
                new_wage,
                dividend,
            }
        };

        let sells = state.businesses.get(&bid).map(|b| b.sells);
        if let Some(b) = state.businesses.get_mut(&bid) {
            b.last_window_profit = plan.window_profit;
            b.dry_windows = plan.dry_windows;
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
            // Workers notice which way their pay moved.
            let staff_and_owner = state
                .businesses
                .get(&bid)
                .map(|b| (b.workers.clone(), b.owner));
            if let Some((staff, wage_owner)) = staff_and_owner {
                for aid in staff {
                    if let Some(a) = state.agents.get_mut(&aid) {
                        crate::relationships::on_wage_moved(a, wage_owner, new > old);
                        crate::reputation::on_wage_moved_witness(a, wage_owner, new > old);
                    }
                }
            }
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
            if let Some(b) = state.businesses.get_mut(&bid) {
                b.books.dividends += amount;
            }
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
    fn contract_committed_stock_does_not_read_as_glut() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let ids: Vec<BusinessId> = w.state.businesses.keys().copied().collect();
        let (farm, mill) = (ids[0], ids[2]);
        let t = review_tick_for(farm);
        let old = {
            let b = w.state.businesses.get_mut(&farm).unwrap();
            b.stockout_days = 0;
            let sells = b.sells;
            b.sales_ema_milli = 1_000; // expects 1/day
            b.inventory.clear();
            b.add_stock(sells, 100); // would be a massive glut…
            b.price
        };
        // …but 97 of it is the rolling buffer for a signed delivery: free
        // stock is 3 days — healthy, not glutted (the utilization cut is
        // profit-gated off by the loss-making window).
        crate::contracts::sign(
            &mut w.state,
            &mut w.journal,
            0,
            crate::contracts::SupplyTerms {
                seller: farm,
                buyer: mill,
                good: crate::goods::Good::Wheat,
                qty: 97,
                unit_price: Money::from_cents(500),
            },
        );
        {
            let b = w.state.businesses.get_mut(&farm).unwrap();
            b.revenue_window = Money::ZERO;
            b.costs_window = Money::from_cents(100); // loss-making window
        }
        run(&mut w.state, &mut w.journal, t).unwrap();
        assert_eq!(
            w.state.businesses[&farm].price, old,
            "an earmarked delivery buffer must not trigger the glut cut"
        );
    }

    #[test]
    fn idle_capacity_cuts_price_without_glut_or_stockout() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let bid = *w.state.businesses.keys().next().unwrap();
        let t = review_tick_for(bid);
        let old = {
            let b = w.state.businesses.get_mut(&bid).unwrap();
            b.stockout_days = 0;
            b.sales_ema_milli = 1_000; // sells ~1/day
            let sells = b.sells;
            b.inventory.clear();
            b.add_stock(sells, 3); // 3 ≤ 5 × 1: no glut signal
            b.price
        };
        // Farm: 3 workers × 2 batches/worker = capacity 6 ≫ 2 × sales.
        run(&mut w.state, &mut w.journal, t).unwrap();
        assert!(
            w.state.businesses[&bid].price < old,
            "idle capacity must cut price to chase volume"
        );
    }

    #[test]
    fn cash_crunch_fires_last_hired_daily() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let bid = *w.state.businesses.keys().next().unwrap();
        let last_hired = *w.state.businesses[&bid].workers.last().unwrap();
        let staff_before = w.state.businesses[&bid].workers.len();
        let owner = w.state.businesses[&bid].owner;
        w.state.businesses.get_mut(&bid).unwrap().cash = Money::from_cents(100);
        // The owner is broke too, so no capital injection can rescue
        // payroll — and the bank is drained, so credit can't either (the
        // rescue-by-loan path has its own test).
        w.state.agents.get_mut(&owner).unwrap().cash = Money::from_cents(500);
        w.state.bank.cash = Money::ZERO;
        w.state.bank.books = crate::bank::BankBooks::new(Money::ZERO);
        w.state.expected_total_money = w.state.total_cash();
        // Non-review tick: only the daily emergency rule runs.
        let t = review_tick_for(bid) + 1;
        run(&mut w.state, &mut w.journal, t).unwrap();
        let b = &w.state.businesses[&bid];
        assert_eq!(b.workers.len(), staff_before - 1);
        assert_eq!(w.state.agents[&last_hired].employer, None);
    }

    #[test]
    fn distressed_businesses_borrow_when_the_owner_cannot_inject() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let bid = *w.state.businesses.keys().next().unwrap();
        let owner = w.state.businesses[&bid].owner;
        let staff_before = w.state.businesses[&bid].workers.len();
        {
            let b = w.state.businesses.get_mut(&bid).unwrap();
            b.cash = Money::from_cents(100);
            b.books = crate::business::Books::new(Money::from_cents(100));
        }
        {
            let o = w.state.agents.get_mut(&owner).unwrap();
            o.cash = Money::from_cents(500); // below the injection reserve
            o.traits.risk_tolerance = 80; // no debt aversion in the way
        }
        w.state.expected_total_money = w.state.total_cash();
        let t = review_tick_for(bid) + 1;
        run(&mut w.state, &mut w.journal, t).unwrap();
        let loan = w
            .state
            .bank
            .active_loan_of(bid)
            .expect("the bank stepped in where the owner could not");
        assert!(loan.principal >= crate::bank::MIN_LOAN);
        assert!(
            w.state.businesses[&bid].cash > Money::from_cents(100),
            "the loan recapitalized the till"
        );
        assert_eq!(
            w.state.businesses[&bid].workers.len(),
            staff_before,
            "credit spared the last hire"
        );
        assert_eq!(w.state.businesses[&bid].books.loan_received, loan.principal);
        assert!(w.journal.decisions.iter().any(|d| matches!(
            &d.detail,
            decision::DecisionDetail::BorrowReview {
                business,
                chosen: decision::BorrowAction::Borrow,
                refused: None,
                ..
            } if *business == bid
        )));
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(e.event, Event::LoanIssued { business, .. } if business == bid)));
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        crate::invariants::check_all(&w.state, &w.journal).unwrap();
    }

    #[test]
    fn a_punitive_rate_deters_all_but_the_desperate() {
        // The probe_rate_shock micro-foundation: identical distress,
        // different rates, different choices.
        let cheap = decision::BorrowInputs {
            payroll_days_left: 2,
            rate_bp: 1_800,
            risk_tolerance: 50,
        };
        let (a, _) = decision::choose_borrow_action(&cheap);
        assert_eq!(a, decision::BorrowAction::Borrow);
        let dear = decision::BorrowInputs {
            payroll_days_left: 2,
            rate_bp: 15_000,
            risk_tolerance: 50,
        };
        let (a, _) = decision::choose_borrow_action(&dear);
        assert_eq!(a, decision::BorrowAction::Struggle, "150% deters");
        // …unless payroll fails tomorrow and the owner is bold.
        let desperate = decision::BorrowInputs {
            payroll_days_left: 0,
            rate_bp: 15_000,
            risk_tolerance: 100,
        };
        let (a, _) = decision::choose_borrow_action(&desperate);
        assert_eq!(a, decision::BorrowAction::Borrow);
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
    fn wealthy_entrepreneur_takes_over_a_moribund_business_and_recapitalizes() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        // Nobody else bids; only the buyer has the appetite.
        for a in w.state.agents.values_mut() {
            a.traits.ambition = 0;
            a.traits.risk_tolerance = 0;
        }
        let bid = *w.state.businesses.keys().next().unwrap();
        let seller = w.state.businesses[&bid].owner;
        // Make the farm moribund: no staff, no cash, broke owner.
        let staff: Vec<AgentId> = w.state.businesses[&bid].workers.clone();
        for aid in staff {
            w.state.agents.get_mut(&aid).unwrap().employer = None;
        }
        {
            let b = w.state.businesses.get_mut(&bid).unwrap();
            b.workers.clear();
            b.cash = Money::from_cents(100);
            b.books = crate::business::Books::new(Money::from_cents(100));
        }
        w.state.agents.get_mut(&seller).unwrap().cash = Money::from_cents(500);
        // The buyer: an ex-worker with savings and fire in the belly.
        let buyer = AgentId(12);
        {
            let a = w.state.agents.get_mut(&buyer).unwrap();
            a.traits.ambition = 100;
            a.traits.risk_tolerance = 100;
            a.cash = Money::from_cents(100_000);
        }
        w.state.expected_total_money = w.state.total_cash();
        let seller_before = w.state.agents[&seller].cash;
        // Tick 2 is id 12's review day.
        run(&mut w.state, &mut w.journal, 2).unwrap();
        assert_eq!(w.state.businesses[&bid].owner, buyer);
        assert_eq!(w.state.agents[&buyer].owns, Some(bid));
        assert_eq!(w.state.agents[&seller].owns, None);
        assert!(
            w.state.agents[&seller].cash > seller_before,
            "the broke seller was paid asset value"
        );
        // Same pass: the new owner recapitalized the firm.
        assert!(w.state.businesses[&bid].cash > Money::from_cents(100));
        assert!(w.state.businesses[&bid].books.owner_invested > Money::ZERO);
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        assert_eq!(
            w.state.businesses[&bid].cash,
            w.state.businesses[&bid].books.expected_cash(),
            "books reconcile through the sale"
        );
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(e.event, Event::BusinessSold { business, .. } if business == bid)));
    }

    #[test]
    fn healthy_businesses_and_timid_money_stay_put() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        for a in w.state.agents.values_mut() {
            a.traits.ambition = 0;
            a.traits.risk_tolerance = 0;
        }
        let bid = *w.state.businesses.keys().next().unwrap();
        let old_owner = w.state.businesses[&bid].owner;
        // Rich but timid: appetite gate fails even against a moribund firm.
        let staff: Vec<AgentId> = w.state.businesses[&bid].workers.clone();
        for aid in staff {
            w.state.agents.get_mut(&aid).unwrap().employer = None;
        }
        {
            let b = w.state.businesses.get_mut(&bid).unwrap();
            b.workers.clear();
            b.cash = Money::from_cents(100);
        }
        w.state.agents.get_mut(&old_owner).unwrap().cash = Money::from_cents(500);
        w.state.agents.get_mut(&AgentId(12)).unwrap().cash = Money::from_cents(100_000);
        w.state.expected_total_money = w.state.total_cash();
        run(&mut w.state, &mut w.journal, 2).unwrap();
        assert_eq!(
            w.state.businesses[&bid].owner, old_owner,
            "no appetite, no deal"
        );
    }

    #[test]
    fn dead_businesses_walk_wages_down_instead_of_ratcheting_up() {
        // Regression (DECISIONS.md #026): a zero-activity business has a
        // window profit of exactly zero; under the old `>= 0` raise rule
        // it bid +5% weekly forever until its wage hit the ceiling, which
        // priced rehiring AND takeover revival out of existence. Now: no
        // raise without strictly positive profit, and an offer the till
        // cannot fund is walked down.
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let bid = *w.state.businesses.keys().next().unwrap();
        let owner = w.state.businesses[&bid].owner;
        let staff: Vec<AgentId> = w.state.businesses[&bid].workers.clone();
        for aid in staff {
            w.state.agents.get_mut(&aid).unwrap().employer = None;
        }
        {
            let b = w.state.businesses.get_mut(&bid).unwrap();
            b.workers.clear();
            b.cash = Money::from_cents(1_000);
            b.books = crate::business::Books::new(Money::from_cents(1_000));
            b.vacancy_days = 30;
            b.revenue_window = Money::ZERO;
            b.costs_window = Money::ZERO;
        }
        // Owner too broke to inject (below the personal reserve).
        w.state.agents.get_mut(&owner).unwrap().cash = Money::from_cents(5_000);
        w.state.expected_total_money = w.state.total_cash();
        let before = w.state.businesses[&bid].wage;
        run(&mut w.state, &mut w.journal, review_tick_for(bid)).unwrap();
        let after = w.state.businesses[&bid].wage;
        assert!(
            after < before,
            "an unfundable offer must fall, not ratchet: {before} -> {after}"
        );
    }

    #[test]
    fn review_signs_with_the_cheapest_capable_seller_once() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let ids: Vec<BusinessId> = w.state.businesses.keys().copied().collect();
        // Two iron-ore sellers: the mine, and a farm converted into a
        // dearer ore vendor. The steelworks is the buyer.
        let (dear_ore, mine, steelworks) = (ids[0], ids[4], ids[5]);
        w.state.businesses.get_mut(&mine).unwrap().price = Money::from_cents(700);
        {
            let b = w.state.businesses.get_mut(&dear_ore).unwrap();
            b.sells = crate::goods::Good::IronOre;
            b.price = Money::from_cents(750);
        }
        let owner = w.state.businesses[&steelworks].owner;
        {
            let o = w.state.agents.get_mut(&owner).unwrap();
            o.traits.risk_tolerance = 50;
            o.traits.greed = 50;
        }
        // Pin the mine owner's greed so the haggle is fully determined by
        // the test, not the worldgen roll.
        let mine_owner = w.state.businesses[&mine].owner;
        w.state.agents.get_mut(&mine_owner).unwrap().traits.greed = 50;
        // No seeded cover: supply security matters to a neutral owner.
        w.state
            .businesses
            .get_mut(&steelworks)
            .unwrap()
            .inventory
            .remove(&crate::goods::Good::IronOre);
        let t = review_tick_for(steelworks);
        run(&mut w.state, &mut w.journal, t).unwrap();
        let signed: Vec<_> = w
            .state
            .contracts
            .values()
            .filter(|c| c.buyer == steelworks && c.good == crate::goods::Good::IronOre)
            .collect();
        assert_eq!(signed.len(), 1, "exactly one ore contract");
        let c = signed[0];
        assert_eq!(c.seller, mine, "the cheaper seller wins");
        // The haggle at greed 50/50 on a $7.00 posted price: buyer opens
        // $6.37, seller counters $6.83, split $6.60 misses the $6.65
        // floor, and the buyer takes the bottom line.
        assert_eq!(c.unit_price, Money::from_cents(665));
        assert!(c.qty >= 1);
        // The whole exchange is on the record, ending in a signature.
        let neg = w
            .journal
            .negotiations
            .iter()
            .find(|n| n.buyer == steelworks && n.seller == mine)
            .expect("the table was logged");
        assert!(neg.rounds.len() >= 4, "a real back-and-forth");
        assert!(matches!(
            neg.outcome,
            crate::negotiation::NegotiationOutcome::Signed { .. }
        ));
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(e.event, crate::events::Event::ContractSigned { buyer, .. } if buyer == steelworks)));
        assert!(w.journal.decisions.iter().any(|d| matches!(
            &d.detail,
            decision::DecisionDetail::SupplyReview { business, chosen: decision::ContractAction::Sign, .. } if *business == steelworks
        )));
        // Next review: the active contract blocks a second signing.
        run(&mut w.state, &mut w.journal, t + REVIEW_PERIOD).unwrap();
        let ore_contracts = w
            .state
            .contracts
            .values()
            .filter(|c| c.buyer == steelworks && c.good == crate::goods::Good::IronOre)
            .count();
        assert_eq!(ore_contracts, 1, "no double-signing while active");
    }

    #[test]
    fn gambler_owners_stay_spot_while_covered_and_the_record_says_why() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let ids: Vec<BusinessId> = w.state.businesses.keys().copied().collect();
        let steelworks = ids[5];
        let owner = w.state.businesses[&steelworks].owner;
        {
            let o = w.state.agents.get_mut(&owner).unwrap();
            o.traits.risk_tolerance = 95;
            o.traits.greed = 50;
        }
        // Full input cover: no security pressure.
        {
            let need = market::daily_input_need(
                &w.state.businesses[&steelworks],
                crate::goods::Good::IronOre,
            );
            let b = w.state.businesses.get_mut(&steelworks).unwrap();
            b.inventory.remove(&crate::goods::Good::IronOre);
            b.add_stock(
                crate::goods::Good::IronOre,
                need * market::INPUT_TARGET_DAYS,
            );
        }
        let t = review_tick_for(steelworks);
        run(&mut w.state, &mut w.journal, t).unwrap();
        assert!(
            !w.state
                .contracts
                .values()
                .any(|c| c.buyer == steelworks && c.good == crate::goods::Good::IronOre),
            "a covered gambler keeps gambling"
        );
        let record = w
            .journal
            .decisions
            .iter()
            .find(|d| {
                matches!(
                    &d.detail,
                    decision::DecisionDetail::SupplyReview { business, .. } if *business == steelworks
                )
            })
            .expect("declining is still a journaled decision");
        assert!(record.explanation().contains("Declined"));
        assert!(record.explanation().contains("risk tolerance 95"));
    }

    #[test]
    fn underwater_buyers_walk_away_and_honesty_holds_out_longer() {
        // The mill locked wheat at $5.60 against a $5.32 reservation cap
        // (flour $7.60 × 70%) — 5.3% underwater. A dishonest owner
        // (tolerance 0%) walks; an honest one (tolerance 10%, ceiling
        // $5.85) honors the deal. Identical books, different people.
        let world_with_honesty = |honesty: u8| {
            let mut w = World::from_config(WorldConfig::default_with_seed(4));
            let ids: Vec<BusinessId> = w.state.businesses.keys().copied().collect();
            let (farm, mill) = (ids[0], ids[2]);
            let cid = crate::contracts::sign(
                &mut w.state,
                &mut w.journal,
                0,
                crate::contracts::SupplyTerms {
                    seller: farm,
                    buyer: mill,
                    good: crate::goods::Good::Wheat,
                    qty: 4,
                    unit_price: Money::from_cents(560),
                },
            );
            let owner = w.state.businesses[&mill].owner;
            w.state.agents.get_mut(&owner).unwrap().traits.honesty = honesty;
            let t = review_tick_for(mill);
            run(&mut w.state, &mut w.journal, t).unwrap();
            (w, cid)
        };

        let (w, cid) = world_with_honesty(0);
        assert_eq!(
            w.state.contracts[&cid].state,
            crate::contracts::ContractState::Terminated,
            "5% underwater is past a dishonest owner's patience"
        );
        assert!(
            w.state.contracts[&cid].penalties_paid_total > Money::ZERO,
            "walking away costs the exit penalty"
        );
        assert!(w
            .journal
            .events
            .iter()
            .any(|e| matches!(e.event, crate::events::Event::ContractTerminated { contract, .. } if contract == cid)));
        assert!(w.journal.decisions.iter().any(|d| matches!(
            &d.detail,
            decision::DecisionDetail::ContractExit { contract, .. } if *contract == cid
        )));
        // The jilted seller's owner now doubts the buyer's owner.
        let seller_owner = w.state.businesses[&w.state.contracts[&cid].seller].owner;
        let buyer_owner = w.state.businesses[&w.state.contracts[&cid].buyer].owner;
        assert!(
            crate::reputation::belief_about(&w.state.agents[&seller_owner], buyer_owner).reliable
                < crate::reputation::NEUTRAL
        );
        crate::invariants::check_all(&w.state, &w.journal).unwrap();

        let (w, cid) = world_with_honesty(100);
        assert_eq!(
            w.state.contracts[&cid].state,
            crate::contracts::ContractState::Active,
            "an honest owner tolerates 10% underwater and honors the deal"
        );
    }

    #[test]
    fn publicly_unreliable_sellers_get_no_contracts() {
        let mut w = World::from_config(WorldConfig::default_with_seed(4));
        let ids: Vec<BusinessId> = w.state.businesses.keys().copied().collect();
        let (dear_ore, mine, steelworks) = (ids[0], ids[4], ids[5]);
        w.state.businesses.get_mut(&mine).unwrap().price = Money::from_cents(700);
        {
            let b = w.state.businesses.get_mut(&dear_ore).unwrap();
            b.sells = crate::goods::Good::IronOre;
            b.price = Money::from_cents(750);
        }
        let buyer_owner = w.state.businesses[&steelworks].owner;
        let mine_owner = w.state.businesses[&mine].owner;
        let farm_owner = w.state.businesses[&dear_ore].owner;
        {
            let o = w.state.agents.get_mut(&buyer_owner).unwrap();
            o.traits.risk_tolerance = 50;
            o.traits.greed = 50;
            crate::reputation::believe(o, mine_owner, |b| b.reliable = 10);
        }
        w.state.agents.get_mut(&farm_owner).unwrap().traits.greed = 50;
        w.state
            .businesses
            .get_mut(&steelworks)
            .unwrap()
            .inventory
            .remove(&crate::goods::Good::IronOre);
        let t = review_tick_for(steelworks);
        run(&mut w.state, &mut w.journal, t).unwrap();
        let signed: Vec<_> = w
            .state
            .contracts
            .values()
            .filter(|c| c.buyer == steelworks && c.good == crate::goods::Good::IronOre)
            .collect();
        assert_eq!(signed.len(), 1);
        assert_eq!(
            signed[0].seller, dear_ore,
            "the cheaper but distrusted seller is passed over"
        );
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
