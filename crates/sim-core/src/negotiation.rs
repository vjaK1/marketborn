//! Deterministic negotiation (Phase 3, per BRIEF.md §Contracts and
//! negotiation): offers, counteroffers, and the reason for every move,
//! logged completely.
//!
//! v1 covers supply-contract formation. The haggle is a bounded, integer,
//! three-round protocol anchored entirely in observable state — no RNG:
//!
//! - The buyer opens below the seller's posted price; greed stretches the
//!   opening discount (6%–12% under spot).
//! - The seller holds a reserve floor under its own posted price; greed
//!   narrows how far it will concede (2%–8% under).
//! - Offers converge by explicit rules (meet partway from posted, split
//!   the difference capped by the buyer's input reservation ceiling, then
//!   the seller's bottom line). If the seller's floor sits above what the
//!   input can earn back, the buyer walks: impasse.
//!
//! Every round is a [`NegotiationRound`] with the mover, the price, and
//! the reason; the whole exchange is journaled as a [`NegotiationRecord`]
//! (an output — saved, shown in the contract view's history table, never
//! hashed, never read back by simulation logic). The achieved discount —
//! not a flat constant — feeds the buyer's Sign/StaySpot review, so a
//! stingy seller's meager concession can win the table and still lose the
//! deal (DECISIONS.md #028).

use crate::goods::{Good, Qty};
use crate::ids::{BusinessId, ContractId};
use crate::money::Money;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NegotiationParty {
    Buyer,
    Seller,
}

impl NegotiationParty {
    pub fn label(self) -> &'static str {
        match self {
            NegotiationParty::Buyer => "buyer",
            NegotiationParty::Seller => "seller",
        }
    }
}

/// Why a party moved the way it did — rendered in the history table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NegotiationMove {
    /// Buyer anchors below spot; greed stretches the discount.
    OpeningBid,
    /// Seller meets partway between its posted price and its floor.
    CounterFromPosted,
    /// Buyer splits toward the counter, capped by what the input earns.
    SplitDifference,
    /// Seller's floor: take it or leave it.
    BottomLine,
    /// This party accepted the standing price.
    Accepted,
    /// This party walked away.
    WalkedAway,
}

impl NegotiationMove {
    pub fn label(self) -> &'static str {
        match self {
            NegotiationMove::OpeningBid => "opened below spot",
            NegotiationMove::CounterFromPosted => "countered partway from the posted price",
            NegotiationMove::SplitDifference => "split the difference, capped by earnings",
            NegotiationMove::BottomLine => "gave the bottom line",
            NegotiationMove::Accepted => "accepted",
            NegotiationMove::WalkedAway => "walked away",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NegotiationRound {
    pub by: NegotiationParty,
    pub unit_price: Money,
    pub because: NegotiationMove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NegotiationOutcome {
    /// Terms agreed and the buyer's review signed the contract.
    Signed { contract: ContractId },
    /// Terms agreed at the table, but the buyer's Sign/StaySpot review
    /// judged the achieved discount not worth the commitment.
    BuyerDeclined { unit_price: Money },
    /// The seller's floor sat above what the input can earn back.
    Impasse,
}

/// One complete logged negotiation (an output, never hashed).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NegotiationRecord {
    pub seq: u64,
    pub tick: u64,
    pub buyer: BusinessId,
    pub seller: BusinessId,
    pub good: Good,
    /// Daily ceiling under discussion.
    pub qty: Qty,
    pub rounds: Vec<NegotiationRound>,
    pub outcome: NegotiationOutcome,
}

/// The table's result before the buyer's review rules on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HaggleResult {
    pub rounds: Vec<NegotiationRound>,
    /// `Some(price)` when the table agreed; `None` on impasse.
    pub agreed: Option<Money>,
}

/// Basis points of opening discount the buyer anchors with: 6%–12% under
/// the posted price by greed.
fn opening_discount_bp(buyer_greed: u8) -> i64 {
    600 + i64::from(buyer_greed) * 6
}

/// Basis points under its posted price the seller will concede at most:
/// 2%–8%, narrower the greedier the seller.
fn floor_discount_bp(seller_greed: u8) -> i64 {
    200 + i64::from(100 - seller_greed) * 6
}

/// Run the three-round haggle. `posted` is the seller's posted price,
/// `buyer_cap` the buyer's input reservation ceiling (it never pays
/// above what the input earns back). Deterministic, integer, bounded.
pub fn haggle(posted: Money, buyer_cap: Money, buyer_greed: u8, seller_greed: u8) -> HaggleResult {
    let min_price = Money::from_cents(1);
    let open = (posted - posted.mul_bp(opening_discount_bp(buyer_greed))).max(min_price);
    let floor = (posted - posted.mul_bp(floor_discount_bp(seller_greed))).max(min_price);
    let mut rounds = Vec::with_capacity(6);

    // Round 1 — the buyer opens low.
    let bid = open.min(buyer_cap).max(min_price);
    rounds.push(NegotiationRound {
        by: NegotiationParty::Buyer,
        unit_price: bid,
        because: NegotiationMove::OpeningBid,
    });
    if bid >= floor {
        rounds.push(NegotiationRound {
            by: NegotiationParty::Seller,
            unit_price: bid,
            because: NegotiationMove::Accepted,
        });
        return HaggleResult {
            rounds,
            agreed: Some(bid),
        };
    }
    // Seller counters partway between posted and its floor.
    let counter = posted - Money::from_cents((posted - floor).cents() / 2);
    rounds.push(NegotiationRound {
        by: NegotiationParty::Seller,
        unit_price: counter,
        because: NegotiationMove::CounterFromPosted,
    });

    // Round 2 — the buyer splits the difference, capped by its ceiling.
    let split = Money::from_cents((bid.cents() + counter.cents()) / 2).min(buyer_cap);
    rounds.push(NegotiationRound {
        by: NegotiationParty::Buyer,
        unit_price: split,
        because: NegotiationMove::SplitDifference,
    });
    if split >= floor {
        rounds.push(NegotiationRound {
            by: NegotiationParty::Seller,
            unit_price: split,
            because: NegotiationMove::Accepted,
        });
        return HaggleResult {
            rounds,
            agreed: Some(split),
        };
    }
    // Seller's bottom line.
    rounds.push(NegotiationRound {
        by: NegotiationParty::Seller,
        unit_price: floor,
        because: NegotiationMove::BottomLine,
    });

    // Round 3 — the buyer takes the floor if the input can earn it back.
    if floor <= buyer_cap {
        rounds.push(NegotiationRound {
            by: NegotiationParty::Buyer,
            unit_price: floor,
            because: NegotiationMove::Accepted,
        });
        HaggleResult {
            rounds,
            agreed: Some(floor),
        }
    } else {
        rounds.push(NegotiationRound {
            by: NegotiationParty::Buyer,
            unit_price: floor,
            because: NegotiationMove::WalkedAway,
        });
        HaggleResult {
            rounds,
            agreed: None,
        }
    }
}

/// The achieved discount off the posted price, in basis points (what the
/// buyer's Sign/StaySpot review weighs).
pub fn achieved_discount_bp(posted: Money, agreed: Money) -> i64 {
    if posted.cents() <= 0 {
        return 0;
    }
    (posted - agreed).cents() * 10_000 / posted.cents()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cents(c: i64) -> Money {
        Money::from_cents(c)
    }

    #[test]
    fn a_generous_seller_takes_the_opening_bid() {
        // Greed 0 seller: floor 8% under posted. Greed 0 buyer opens 6%
        // under — above the floor, done in one round.
        let r = haggle(cents(1_000), cents(2_000), 0, 0);
        assert_eq!(r.agreed, Some(cents(940)));
        assert_eq!(r.rounds.len(), 2);
        assert_eq!(r.rounds[1].because, NegotiationMove::Accepted);
        assert_eq!(r.rounds[1].by, NegotiationParty::Seller);
    }

    #[test]
    fn greedy_parties_grind_to_the_bottom_line() {
        // Greedy buyer opens 12% under; greedy seller floors at 2% under:
        // open 880 < floor 980, counter 990, split 935 < floor, bottom
        // line 980 ≤ cap — agreed at the seller's floor after 3 rounds.
        let r = haggle(cents(1_000), cents(2_000), 100, 100);
        assert_eq!(r.agreed, Some(cents(980)));
        assert_eq!(r.rounds.len(), 5);
        assert_eq!(r.rounds[2].because, NegotiationMove::SplitDifference);
        assert_eq!(r.rounds[3].because, NegotiationMove::BottomLine);
        assert_eq!(r.rounds[4].because, NegotiationMove::Accepted);
        assert_eq!(r.rounds[4].by, NegotiationParty::Buyer);
    }

    #[test]
    fn a_floor_above_the_buyers_ceiling_is_an_impasse() {
        // The seller's 2%-under floor (980) exceeds what the input earns
        // back (900): the buyer walks and the table logs it.
        let r = haggle(cents(1_000), cents(900), 100, 100);
        assert_eq!(r.agreed, None);
        assert_eq!(
            r.rounds.last().unwrap().because,
            NegotiationMove::WalkedAway
        );
        assert_eq!(r.rounds.last().unwrap().by, NegotiationParty::Buyer);
    }

    #[test]
    fn traits_move_real_money_and_the_discount_reflects_it() {
        let posted = cents(1_000);
        let generous = haggle(posted, cents(2_000), 50, 0).agreed.unwrap();
        let stingy = haggle(posted, cents(2_000), 50, 100).agreed.unwrap();
        assert!(
            generous < stingy,
            "a generous seller concedes more: {generous} vs {stingy}"
        );
        assert!(achieved_discount_bp(posted, generous) > achieved_discount_bp(posted, stingy));
        // Identical inputs, identical outcome — determinism.
        assert_eq!(
            haggle(posted, cents(2_000), 50, 0),
            haggle(posted, cents(2_000), 50, 0)
        );
    }
}
