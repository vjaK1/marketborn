//! The utility-based decision engine (Phase 2, per AGENT_DESIGN.md).
//!
//! Actions are enumerated, scored, and the best is chosen; ties break by
//! action enum order. **Utility scores are the one sanctioned float zone**
//! (CLAUDE.md): they order choices, and every executed consequence goes
//! back through integer ledgers. Traits weight the scores — they bias
//! choices under conflicting signals, never fully determine them.
//!
//! Every choice is journaled as a [`DecisionRecord`] with the scores and
//! the inputs that produced them, rendered on demand for the agent
//! inspector ("why did you do that?"). Records are outputs: saved, never
//! hashed, never read back by simulation logic.
//!
//! Phase 2 rollout starts with the business price review; further actions
//! (job switching, entry/exit, negotiation) join incrementally.

use crate::agent::Traits;
use crate::goods::{Good, Qty};
use crate::ids::{AgentId, BusinessId};
use crate::money::Money;
use serde::{Deserialize, Serialize};

/// Price-review actions, in tie-break order (earlier wins on equal score —
/// mirrors the old rule cascade's priority).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriceAction {
    Raise,
    CutHeavy,
    CutLight,
    Hold,
}

impl PriceAction {
    pub const ALL: [PriceAction; 4] = [
        PriceAction::Raise,
        PriceAction::CutHeavy,
        PriceAction::CutLight,
        PriceAction::Hold,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PriceAction::Raise => "raise the price",
            PriceAction::CutHeavy => "cut the price hard",
            PriceAction::CutLight => "cut the price",
            PriceAction::Hold => "hold the price",
        }
    }
}

/// The signals a price review reads, captured verbatim into the record.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PriceInputs {
    pub stockout_days: u32,
    /// Days of stock on hand at expected sales (stock / max(ema, 1)).
    pub stock_days: Qty,
    /// Expected daily sales vs bare-handed capacity, in percent (100 =
    /// selling exactly capacity). 0 when the business has no workers.
    pub utilization_pct: Qty,
    pub window_profitable: bool,
    /// Consecutive completed review windows (including this one) with zero
    /// revenue. One is normal duopoly alternation; a run of them means the
    /// posted price earns nothing.
    pub dry_windows: u32,
    /// The deciding owner's weighting traits.
    pub greed: u8,
    pub aggression: u8,
}

/// Zero-revenue windows tolerated before the deadlock breaker fires.
pub const DRY_WINDOWS_BREAKER: u32 = 3;

/// Utility scores for the price review. Neutral traits (50) reproduce the
/// Phase 0/1 rule family: raise on ≥2 stockout days; cut hard above 8 days
/// of stock; cut above 6 days or below 50% utilization from strength.
/// Greed weights the raise urge; aggression resists volume-chasing cuts.
pub fn score_price_action(action: PriceAction, i: &PriceInputs) -> f64 {
    // Traits act two ways: multiplicative weights settle conflicts between
    // competing signals, and bounded THRESHOLD shifts make personality
    // matter on ordinary days too (a pure weight never flips a
    // single-signal choice against Hold's zero).
    let greed_w = 0.5 + f64::from(i.greed) / 100.0; // 0.5 ..= 1.5
    match action {
        // Threshold bands are deliberately narrow — traits decide the
        // AMBIGUOUS calls (marginal utilization, conflicting signals),
        // never the clear ones. Wider bands proved destabilizing: timid
        // owners cutting at healthy utilization deflated whole towns.
        PriceAction::Raise => {
            // ±0.4 days: inert against integer stockout counts on its own,
            // but tips conflicts alongside the greed weight.
            let threshold = 1.5 - (f64::from(i.greed) - 50.0) / 125.0;
            (f64::from(i.stockout_days) - threshold) * 3.0 * greed_w
        }
        PriceAction::CutHeavy => {
            let glut = i.stock_days as f64 - 8.0;
            // Price-deadlock breaker: a RUN of zero-revenue windows while
            // holding stock (or staffed to produce) means the posted price
            // earns exactly nothing — cutting cannot reduce revenue below
            // zero, so the profit gate does not apply. Without it,
            // post-spike towns froze at unaffordable prices forever while
            // every corrective signal stayed silent. The run length
            // matters: firing on a single quiet week turned normal duopoly
            // alternation into leapfrogging price wars that razed every
            // town (DECISIONS.md #022).
            let dead_market =
                i.dry_windows >= DRY_WINDOWS_BREAKER && (i.stock_days > 0 || i.utilization_pct > 0);
            if dead_market {
                glut.max(2.0)
            } else {
                glut
            }
        }
        PriceAction::CutLight => {
            // Capped so a deepening glut escalates to the heavy cut
            // instead of scoring the light one ever higher.
            let glut = ((i.stock_days as f64 - 6.0) * 0.5).min(1.0);
            // Aggressive owners tolerate a little more idle capacity to
            // defend margin (cut below 42%); timid ones chase volume a
            // little sooner (below 58%). Neutral: 50%.
            let idle = if i.window_profitable && i.utilization_pct > 0 {
                let threshold = 0.5 + (50.0 - f64::from(i.aggression)) / 625.0;
                threshold - i.utilization_pct as f64 / 100.0
            } else {
                f64::MIN
            };
            glut.max(idle)
        }
        PriceAction::Hold => 0.0,
    }
}

/// Score all actions and pick the winner (ties break by enum order).
pub fn choose_price_action(inputs: &PriceInputs) -> (PriceAction, Vec<ScoredAction>) {
    let mut best = PriceAction::Hold;
    let mut best_score = f64::MIN;
    let mut considered = Vec::with_capacity(PriceAction::ALL.len());
    for action in PriceAction::ALL {
        let score = score_price_action(action, inputs);
        considered.push(ScoredAction { action, score });
        if score > best_score {
            best = action;
            best_score = score;
        }
    }
    (best, considered)
}

pub fn price_inputs(
    stockout_days: u32,
    stock: Qty,
    ema_day: Qty,
    base_capacity_units: Qty,
    window_profitable: bool,
    dry_windows: u32,
    traits: Traits,
) -> PriceInputs {
    let ema = ema_day.max(1);
    PriceInputs {
        stockout_days,
        stock_days: stock / ema,
        utilization_pct: if base_capacity_units > 0 {
            ema * 100 / base_capacity_units
        } else {
            0
        },
        window_profitable,
        dry_windows,
        greed: traits.greed,
        aggression: traits.aggression,
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ScoredAction {
    pub action: PriceAction,
    /// Journal-only float (never hashed, never read back by sim logic).
    pub score: f64,
}

/// A job with its posted wage, as seen at decision time.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct JobRef {
    pub business: BusinessId,
    pub wage: Money,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobAction {
    /// Keep the current job (no rival offer cleared the loyalty premium).
    Stay,
    /// Quit the current job for a better-paying vacancy.
    SwitchTo(BusinessId),
    /// Unemployed and holding out: every open wage is below reservation.
    HoldOut,
}

/// The wage an unemployed agent holds out for: 0.5–1.5× the going food
/// price depending on ambition, **decaying linearly with unemployment
/// duration** over a patience-scaled horizon (30–90 days) — pride is a
/// wasting asset, so a holdout can never permanently block restaffing (a
/// non-decaying reservation let comfortable workers watch their would-be
/// employer die — the seed-42 lesson, DECISIONS.md #020). Desperation
/// (hunger, or savings below a month of food) drops it to zero at once.
/// The division rounds toward zero, slightly favoring employment.
pub fn reservation_wage(
    food_price: Money,
    ambition: u8,
    patience: u8,
    days_unemployed: u32,
    desperate: bool,
) -> Money {
    if desperate {
        return Money::ZERO;
    }
    let base = food_price.mul_bp(5_000 + i64::from(ambition) * 100);
    let horizon = 30 + i64::from(patience) * 60 / 100; // 30 ..= 90 days
    let left = (horizon - i64::from(days_unemployed)).max(0);
    Money::from_cents(base.cents() * left / horizon)
}

/// The raise premium a rival job must clear before a worker switches:
/// 10%–20% of the current wage, widened by loyalty.
pub fn switch_premium_bp(loyalty: u8) -> i64 {
    1_000 + i64::from(loyalty) * 10
}

/// Whether an agent has the entrepreneurial appetite to take over a
/// moribund business: ambition plus risk tolerance must clear 120 —
/// roughly a third of people. Wealth does the real rationing; this gate
/// makes WHO founds things a matter of personality.
pub fn takeover_appetite(ambition: u8, risk_tolerance: u8) -> bool {
    u32::from(ambition) + u32::from(risk_tolerance) > 120
}

/// Supply-contract review actions, in tie-break order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractAction {
    /// Lock the recurring delivery at the discounted price.
    Sign,
    /// Keep buying day to day on the spot market.
    StaySpot,
}

impl ContractAction {
    pub const ALL: [ContractAction; 2] = [ContractAction::Sign, ContractAction::StaySpot];

    pub fn label(self) -> &'static str {
        match self {
            ContractAction::Sign => "sign the supply contract",
            ContractAction::StaySpot => "stay on the spot market",
        }
    }
}

/// The signals a supply-contract review reads, captured into the record.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SupplyContractInputs {
    /// Discount off the seller's posted price, in basis points.
    pub discount_bp: i64,
    /// Days of this input on hand at the expected consumption rate.
    pub cover_days: Qty,
    /// The deciding owner's weighting traits.
    pub greed: u8,
    pub risk_tolerance: u8,
}

/// Utility scores for signing a supply contract versus staying spot.
/// Certainty is the axis: cautious owners buy supply security early,
/// gamblers hold out on the spot market until their input cover actually
/// thins, and greed weighs the locked-in discount. Neutral owners sign —
/// recurring contracts are ordinary business, refusing them is the
/// personality-driven exception (DECISIONS.md #026).
pub fn score_contract_action(action: ContractAction, i: &SupplyContractInputs) -> f64 {
    match action {
        ContractAction::Sign => {
            let greed_w = 0.5 + f64::from(i.greed) / 100.0; // 0.5 ..= 1.5
            let caution = (100.0 - f64::from(i.risk_tolerance)) / 100.0; // 0 ..= 1
            let discount = (i.discount_bp as f64 / 100.0) * 0.1 * greed_w;
            let security =
                (crate::market::INPUT_TARGET_DAYS - i.cover_days) as f64 * 0.3 * (0.5 + caution);
            let rt = f64::from(i.risk_tolerance) / 100.0;
            let commitment = rt * rt * 1.2; // gamblers hate being locked in
            discount + security - commitment
        }
        ContractAction::StaySpot => 0.0,
    }
}

/// Score both actions and pick the winner (ties break by enum order).
pub fn choose_contract_action(
    inputs: &SupplyContractInputs,
) -> (ContractAction, Vec<ScoredContractAction>) {
    let mut best = ContractAction::StaySpot;
    let mut best_score = f64::MIN;
    let mut considered = Vec::with_capacity(ContractAction::ALL.len());
    for action in ContractAction::ALL {
        let score = score_contract_action(action, inputs);
        considered.push(ScoredContractAction { action, score });
        if score > best_score {
            best = action;
            best_score = score;
        }
    }
    (best, considered)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ScoredContractAction {
    pub action: ContractAction,
    /// Journal-only float (never hashed, never read back by sim logic).
    pub score: f64,
}

/// Borrow-review actions, in tie-break order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BorrowAction {
    /// Take the bank's working-capital loan at the current rate.
    Borrow,
    /// Ride out the shortfall without debt.
    Struggle,
}

impl BorrowAction {
    pub const ALL: [BorrowAction; 2] = [BorrowAction::Borrow, BorrowAction::Struggle];

    pub fn label(self) -> &'static str {
        match self {
            BorrowAction::Borrow => "borrow from the bank",
            BorrowAction::Struggle => "struggle through without debt",
        }
    }
}

/// The signals a borrow review reads, captured into the record.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BorrowInputs {
    /// Days of payroll the business can still cover from cash.
    pub payroll_days_left: Qty,
    /// The bank's current annual base rate in basis points.
    pub rate_bp: i64,
    /// The deciding owner's weighting trait: debt appetite.
    pub risk_tolerance: u8,
}

/// Utility scores for borrowing against struggling on. Urgency is runway:
/// the closer payroll failure looms, the stronger the pull; the rate is
/// the push, weighed heavier by debt-averse (low risk tolerance) owners.
/// This price sensitivity is the transmission channel `probe_rate_shock`
/// guards: at a punitive rate only the truly desperate and the bold still
/// borrow (DECISIONS.md #027).
pub fn score_borrow_action(action: BorrowAction, i: &BorrowInputs) -> f64 {
    match action {
        BorrowAction::Borrow => {
            let urgency = (5.0 - i.payroll_days_left as f64).max(0.0) / 5.0; // 0 ..= 1
            let caution = (100.0 - f64::from(i.risk_tolerance)) / 100.0; // 0 ..= 1
            let cost = (i.rate_bp as f64 / 10_000.0) * (0.5 + caution);
            urgency * 1.2 - cost
        }
        BorrowAction::Struggle => 0.0,
    }
}

/// Score both actions and pick the winner (ties break by enum order).
pub fn choose_borrow_action(inputs: &BorrowInputs) -> (BorrowAction, Vec<ScoredBorrowAction>) {
    let mut best = BorrowAction::Struggle;
    let mut best_score = f64::MIN;
    let mut considered = Vec::with_capacity(BorrowAction::ALL.len());
    for action in BorrowAction::ALL {
        let score = score_borrow_action(action, inputs);
        considered.push(ScoredBorrowAction { action, score });
        if score > best_score {
            best = action;
            best_score = score;
        }
    }
    (best, considered)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ScoredBorrowAction {
    pub action: BorrowAction,
    /// Journal-only float (never hashed, never read back by sim logic).
    pub score: f64,
}

/// One journaled decision: who chose what, every score considered, and the
/// inputs that mattered.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub seq: u64,
    pub tick: u64,
    pub actor: AgentId,
    pub detail: DecisionDetail,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DecisionDetail {
    PriceReview {
        business: BusinessId,
        inputs: PriceInputs,
        considered: Vec<ScoredAction>,
        chosen: PriceAction,
    },
    JobReview {
        current: Option<JobRef>,
        best_open: Option<JobRef>,
        reservation: Money,
        premium_bp: i64,
        chosen: JobAction,
    },
    /// A wealthy agent acquired a moribund business from its broke owner
    /// (DECISIONS.md #021). Recorded only when it happens — passing on a
    /// dead business is the default, not a decision worth journaling.
    Takeover {
        business: BusinessId,
        seller: AgentId,
        price: Money,
        capital_after: Money,
    },
    /// A buyer weighed locking a recurring input delivery against staying
    /// on the spot market (Phase 3). Journaled whether they sign or not —
    /// staying spot is a real choice about a live offer.
    SupplyReview {
        business: BusinessId,
        seller: BusinessId,
        good: Good,
        qty: Qty,
        unit_price: Money,
        inputs: SupplyContractInputs,
        considered: Vec<ScoredContractAction>,
        chosen: ContractAction,
    },
    /// A distressed owner weighed bank credit against struggling on
    /// (Phase 3). Journaled when the loan is taken, and — to bound the
    /// journal — on the weekly stagger when a live offer is declined or
    /// the bank refuses.
    BorrowReview {
        business: BusinessId,
        amount: Money,
        inputs: BorrowInputs,
        considered: Vec<ScoredBorrowAction>,
        chosen: BorrowAction,
        /// The bank's answer when the owner chose to borrow (`None` until
        /// asked). A refusal reason means the owner wanted the loan and
        /// the bank said no.
        refused: Option<crate::bank::CreditRefusal>,
    },
    /// A buyer walked away from an underwater supply contract: the locked
    /// price ran past what the input can earn back (the reservation cap),
    /// beyond even the owner's honesty-widened tolerance. Recorded only
    /// when it happens — honoring a workable deal is the default.
    ContractExit {
        business: BusinessId,
        contract: crate::ids::ContractId,
        good: Good,
        unit_price: Money,
        /// The buyer's current input reservation cap.
        cap: Money,
        /// How far past the cap the owner would have tolerated, in basis
        /// points (scales with honesty).
        tolerance_bp: i64,
        penalty: Money,
    },
}

impl DecisionRecord {
    /// Human-readable "why": rendered for the agent inspector.
    pub fn explanation(&self) -> String {
        match &self.detail {
            DecisionDetail::PriceReview {
                inputs: i,
                considered,
                chosen,
                ..
            } => {
                let scores = considered
                    .iter()
                    .map(|s| format!("{} {:+.2}", s.action.label(), s.score))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "Chose to {}: {} stockout day(s), {} day(s) of stock, {}% of capacity selling, window {} — weighing greed {} and aggression {}. Scores: {scores}.",
                    chosen.label(),
                    i.stockout_days,
                    i.stock_days,
                    i.utilization_pct,
                    if i.window_profitable {
                        "profitable"
                    } else {
                        "loss-making"
                    },
                    i.greed,
                    i.aggression,
                )
            }
            DecisionDetail::JobReview {
                current,
                best_open,
                reservation,
                premium_bp,
                chosen,
            } => {
                let cur = current
                    .map(|j| format!("earning {} at {}", j.wage, j.business))
                    .unwrap_or_else(|| "unemployed".to_string());
                let open = best_open
                    .map(|j| format!("best open job pays {} at {}", j.wage, j.business))
                    .unwrap_or_else(|| "no open jobs".to_string());
                match chosen {
                    JobAction::Stay => format!(
                        "Stayed put: {cur}; {open} — below the +{}% switch premium.",
                        premium_bp / 100
                    ),
                    JobAction::SwitchTo(b) => {
                        format!(
                            "Switched to {b}: {cur}; {open} — cleared the +{}% premium.",
                            premium_bp / 100
                        )
                    }
                    JobAction::HoldOut => {
                        format!("Held out: {open}, below the {reservation} reservation wage.")
                    }
                }
            }
            DecisionDetail::Takeover {
                business,
                seller,
                price,
                capital_after,
            } => format!(
                "Bought the moribund {business} from {seller} for {price}, keeping {capital_after} to restart it."
            ),
            DecisionDetail::SupplyReview {
                seller,
                good,
                qty,
                unit_price,
                inputs: i,
                considered,
                chosen,
                ..
            } => {
                let scores = considered
                    .iter()
                    .map(|s| format!("{} {:+.2}", s.action.label(), s.score))
                    .collect::<Vec<_>>()
                    .join(", ");
                let verb = match chosen {
                    ContractAction::Sign => "Signed with",
                    ContractAction::StaySpot => "Declined a contract with",
                };
                format!(
                    "{verb} {seller} for {qty} {good}/day at {unit_price} ({}% under spot): {} day(s) of cover on hand — weighing greed {} and risk tolerance {}. Scores: {scores}.",
                    i.discount_bp / 100,
                    i.cover_days,
                    i.greed,
                    i.risk_tolerance,
                )
            }
            DecisionDetail::BorrowReview {
                business,
                amount,
                inputs: i,
                considered,
                chosen,
                refused,
            } => {
                let scores = considered
                    .iter()
                    .map(|s| format!("{} {:+.2}", s.action.label(), s.score))
                    .collect::<Vec<_>>()
                    .join(", ");
                let outcome = match (chosen, refused) {
                    (BorrowAction::Borrow, None) => format!("Borrowed {amount} for {business}"),
                    (BorrowAction::Borrow, Some(r)) => format!(
                        "Asked the bank for {amount} for {business} and was refused: {}",
                        r.label()
                    ),
                    (BorrowAction::Struggle, _) => {
                        format!("Declined credit for {business}")
                    }
                };
                format!(
                    "{outcome}: {} day(s) of payroll left at a {}% rate — weighing risk tolerance {}. Scores: {scores}.",
                    i.payroll_days_left,
                    i.rate_bp / 100,
                    i.risk_tolerance,
                )
            }
            DecisionDetail::ContractExit {
                contract,
                good,
                unit_price,
                cap,
                tolerance_bp,
                penalty,
                ..
            } => format!(
                "Walked away from {contract}: paying {unit_price} per {good} against a {cap} ceiling — past even a {}% tolerance for honoring the deal. Ate the {penalty} exit penalty.",
                tolerance_bp / 100,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral(stockout_days: u32, stock_days: Qty, utilization_pct: Qty) -> PriceInputs {
        PriceInputs {
            stockout_days,
            stock_days,
            utilization_pct,
            window_profitable: true,
            dry_windows: 0,
            greed: 50,
            aggression: 50,
        }
    }

    #[test]
    fn neutral_traits_reproduce_the_rule_family() {
        // Sold out twice with lean stock: raise.
        let (a, _) = choose_price_action(&neutral(2, 1, 80));
        assert_eq!(a, PriceAction::Raise);
        // Deep glut: heavy cut.
        let (a, _) = choose_price_action(&neutral(0, 12, 80));
        assert_eq!(a, PriceAction::CutHeavy);
        // Mild glut: light cut.
        let (a, _) = choose_price_action(&neutral(0, 7, 80));
        assert_eq!(a, PriceAction::CutLight);
        // Idle capacity from strength: light cut.
        let (a, _) = choose_price_action(&neutral(0, 2, 30));
        assert_eq!(a, PriceAction::CutLight);
        // Healthy: hold.
        let (a, _) = choose_price_action(&neutral(0, 3, 80));
        assert_eq!(a, PriceAction::Hold);
    }

    #[test]
    fn loss_making_businesses_do_not_cut_for_volume() {
        let mut i = neutral(0, 2, 30);
        i.window_profitable = false;
        let (a, _) = choose_price_action(&i);
        assert_eq!(a, PriceAction::Hold, "no pricing below cost from weakness");
    }

    #[test]
    fn a_run_of_dry_windows_breaks_the_price_deadlock() {
        // Three straight zero-revenue windows with stock to sell: every
        // other corrective is silent (no stockouts, no glut, idle cut
        // profit-gated) — but a price earning nothing for weeks must fall.
        let mut i = neutral(0, 2, 30);
        i.window_profitable = false;
        i.dry_windows = DRY_WINDOWS_BREAKER;
        let (a, _) = choose_price_action(&i);
        assert_eq!(a, PriceAction::CutHeavy, "weeks of zero revenue: cut");
        // One quiet week is normal duopoly alternation: hold.
        let mut quiet = neutral(0, 2, 30);
        quiet.window_profitable = false;
        quiet.dry_windows = 1;
        let (a, _) = choose_price_action(&quiet);
        assert_eq!(a, PriceAction::Hold, "one dry week is not a signal");
        // With nothing to sell and nobody to make it, hold.
        let mut empty = neutral(0, 0, 0);
        empty.window_profitable = false;
        empty.dry_windows = DRY_WINDOWS_BREAKER;
        let (a, _) = choose_price_action(&empty);
        assert_eq!(a, PriceAction::Hold);
    }

    #[test]
    fn traits_diverge_choices_under_identical_conditions() {
        // Conflicting signals: sold out twice earlier in the window, now
        // sitting on nine days of stock.
        let mut greedy = neutral(2, 9, 80);
        greedy.greed = 90;
        greedy.aggression = 90;
        let mut timid = neutral(2, 9, 80);
        timid.greed = 10;
        timid.aggression = 10;
        let (a_greedy, _) = choose_price_action(&greedy);
        let (a_timid, _) = choose_price_action(&timid);
        assert_eq!(a_greedy, PriceAction::Raise);
        assert_eq!(a_timid, PriceAction::CutHeavy);
        assert_ne!(a_greedy, a_timid, "personality must matter");
    }

    #[test]
    fn records_explain_themselves() {
        let inputs = neutral(3, 1, 90);
        let (chosen, considered) = choose_price_action(&inputs);
        let r = DecisionRecord {
            seq: 0,
            tick: 12,
            actor: crate::ids::AgentId(4),
            detail: DecisionDetail::PriceReview {
                business: crate::ids::BusinessId(4),
                inputs,
                considered,
                chosen,
            },
        };
        let text = r.explanation();
        assert!(text.contains("raise the price"));
        assert!(text.contains("3 stockout day(s)"));
        assert!(text.contains("greed 50"));
    }

    #[test]
    fn reservation_wage_scales_with_ambition_and_yields_to_desperation() {
        let food = Money::from_cents(800);
        assert_eq!(
            reservation_wage(food, 0, 50, 0, false),
            Money::from_cents(400)
        );
        assert_eq!(
            reservation_wage(food, 50, 50, 0, false),
            Money::from_cents(800)
        );
        assert_eq!(
            reservation_wage(food, 100, 50, 0, false),
            Money::from_cents(1_200)
        );
        assert_eq!(reservation_wage(food, 100, 50, 0, true), Money::ZERO);
    }

    #[test]
    fn reservation_wage_decays_to_zero_over_the_patience_horizon() {
        let food = Money::from_cents(800);
        // Patience 50 → 60-day horizon.
        let fresh = reservation_wage(food, 50, 50, 0, false);
        let month = reservation_wage(food, 50, 50, 30, false);
        let done = reservation_wage(food, 50, 50, 60, false);
        assert_eq!(fresh, Money::from_cents(800));
        assert_eq!(
            month,
            Money::from_cents(400),
            "half the horizon, half the pride"
        );
        assert_eq!(done, Money::ZERO, "past the horizon any wage is acceptable");
        // Patience widens the horizon.
        assert!(reservation_wage(food, 50, 100, 60, false) > Money::ZERO);
        assert_eq!(reservation_wage(food, 50, 0, 30, false), Money::ZERO);
    }

    #[test]
    fn contract_appetite_neutral_signs_and_gamblers_wait_for_the_crunch() {
        let at_cover = |risk_tolerance: u8, cover_days: Qty| SupplyContractInputs {
            discount_bp: 500,
            cover_days,
            greed: 50,
            risk_tolerance,
        };
        // A neutral owner takes the discount at full cover.
        let (a, _) = choose_contract_action(&at_cover(50, 3));
        assert_eq!(a, ContractAction::Sign);
        // A cautious owner signs even harder.
        let (a, _) = choose_contract_action(&at_cover(10, 3));
        assert_eq!(a, ContractAction::Sign);
        // A gambler stays spot while well covered…
        let (a, _) = choose_contract_action(&at_cover(90, 3));
        assert_eq!(a, ContractAction::StaySpot);
        // …but signs when supply actually tightens — identical conditions,
        // different people, different timing.
        let (a, _) = choose_contract_action(&at_cover(90, 0));
        assert_eq!(a, ContractAction::Sign);
    }

    #[test]
    fn supply_review_records_explain_themselves() {
        let inputs = SupplyContractInputs {
            discount_bp: 500,
            cover_days: 1,
            greed: 60,
            risk_tolerance: 20,
        };
        let (chosen, considered) = choose_contract_action(&inputs);
        let r = DecisionRecord {
            seq: 0,
            tick: 30,
            actor: crate::ids::AgentId(2),
            detail: DecisionDetail::SupplyReview {
                business: crate::ids::BusinessId(2),
                seller: crate::ids::BusinessId(0),
                good: Good::Wheat,
                qty: 12,
                unit_price: Money::from_cents(523),
                inputs,
                considered,
                chosen,
            },
        };
        let text = r.explanation();
        assert!(text.contains("Signed with B0"), "{text}");
        assert!(text.contains("12 wheat/day"));
        assert!(text.contains("5% under spot"));
        assert!(text.contains("risk tolerance 20"));
    }

    #[test]
    fn switch_premium_widens_with_loyalty() {
        assert_eq!(switch_premium_bp(0), 1_000);
        assert_eq!(switch_premium_bp(50), 1_500);
        assert_eq!(switch_premium_bp(100), 2_000);
        // A 16% raise moves the disloyal but not the loyal — identical
        // conditions, different people, different choices.
        let current = Money::from_cents(700);
        let offer = current + current.mul_bp(1_600);
        let clears = |loyalty: u8| offer >= current + current.mul_bp(switch_premium_bp(loyalty));
        assert!(clears(0));
        assert!(!clears(100));
    }
}
