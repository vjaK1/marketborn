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
use crate::goods::Qty;
use crate::ids::{AgentId, BusinessId};
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
    /// The deciding owner's weighting traits.
    pub greed: u8,
    pub aggression: u8,
}

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
        PriceAction::CutHeavy => i.stock_days as f64 - 8.0,
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

/// One journaled decision: who chose what for which business, every score
/// considered, and the inputs that mattered.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub seq: u64,
    pub tick: u64,
    pub actor: AgentId,
    pub business: BusinessId,
    pub inputs: PriceInputs,
    pub considered: Vec<ScoredAction>,
    pub chosen: PriceAction,
}

impl DecisionRecord {
    /// Human-readable "why": rendered for the agent inspector.
    pub fn explanation(&self) -> String {
        let i = &self.inputs;
        let scores = self
            .considered
            .iter()
            .map(|s| format!("{} {:+.2}", s.action.label(), s.score))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "Chose to {}: {} stockout day(s), {} day(s) of stock, {}% of capacity selling, window {} — weighing greed {} and aggression {}. Scores: {scores}.",
            self.chosen.label(),
            i.stockout_days,
            i.stock_days,
            i.utilization_pct,
            if i.window_profitable { "profitable" } else { "loss-making" },
            i.greed,
            i.aggression,
        )
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
            business: crate::ids::BusinessId(4),
            inputs,
            considered,
            chosen,
        };
        let text = r.explanation();
        assert!(text.contains("raise the price"));
        assert!(text.contains("3 stockout day(s)"));
        assert!(text.contains("greed 50"));
    }
}
