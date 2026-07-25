//! `probe_reputation` — the Phase 2 acceptance probe (TEST_PLAN policy:
//! probes assert that a propagation channel EXISTS, never a scripted
//! outcome; thresholds are calibrated once against this pinned seed and
//! then frozen as regression guards).
//!
//! Scenario surgery (setup only — every propagation step below runs the
//! real machinery): after ten settled days the bakery's cash is zeroed
//! (books resynced), so the next payroll fails and its four workers quit
//! with first-hand beliefs about the owner's unreliability. One farm job
//! is freed for them (its sitting worker is made a comfortable holdout),
//! and the victims' ambitions are zeroed so one takes that job promptly.
//! From there the rumor channel does the rest: weekly workplace gossip
//! seeds the new colleagues' beliefs about a man they never worked for.

use sim_core::{AgentId, Books, BusinessId, Event, Money, World, WorldConfig};

#[test]
fn probe_reputation_bad_news_reaches_strangers_through_gossip() {
    let mut w = World::from_config(WorldConfig::default_with_seed(42));
    w.run_ticks(10).unwrap();

    let bakery = BusinessId(3);
    let bakery_owner = w.state.businesses[&bakery].owner;
    let victims: Vec<AgentId> = w.state.businesses[&bakery].workers.clone();
    assert_eq!(victims.len(), 4, "pinned-seed staffing");

    // Surgery: bankrupt the bakery (cash + books resynced); free one job
    // each at both farms and the mill by turning their sitting workers
    // into comfortable holdouts (gossip needs the victims ON rosters);
    // make the victims willing to take any wage. All propagation from
    // here — the payroll failure, the quitting, the rehiring, the gossip —
    // runs the ordinary machinery.
    {
        let b = w.state.businesses.get_mut(&bakery).unwrap();
        b.cash = Money::ZERO;
        b.books = Books::new(Money::ZERO);
    }
    for host in [BusinessId(0), BusinessId(1), BusinessId(2)] {
        let held_out = w
            .state
            .businesses
            .get_mut(&host)
            .unwrap()
            .workers
            .pop()
            .unwrap();
        let a = w.state.agents.get_mut(&held_out).unwrap();
        a.employer = None;
        a.traits.ambition = 100;
        a.cash = Money::from_cents(100_000);
    }
    for v in &victims {
        w.state.agents.get_mut(v).unwrap().traits.ambition = 0;
    }
    w.state.expected_total_money = w.state.total_cash();

    // Opinions legitimately fade (drift) and compete (gossip both ways),
    // so the channel is asserted on the trajectory: did the knowledge
    // EXIST first-hand, and did it EVER reach a non-witness? Latching
    // day-by-day tests channel existence without scripting persistence.
    let mut firsthand_seen = false;
    let mut stranger_heard = false;
    for _ in 0..50 {
        w.tick().unwrap();
        let firsthand_now = victims
            .iter()
            .filter(|v| {
                sim_core::reputation::belief_about(&w.state.agents[v], bakery_owner).reliable
                    < sim_core::reputation::NEUTRAL
            })
            .count();
        if firsthand_now >= 3 {
            firsthand_seen = true;
        }
        if w.state
            .agents
            .values()
            .filter(|a| a.id != bakery_owner && !victims.contains(&a.id))
            .any(|a| {
                sim_core::reputation::belief_about(a, bakery_owner).reliable
                    < sim_core::reputation::NEUTRAL
            })
        {
            stranger_heard = true;
        }
    }

    // The machinery, not the test, produced the payroll failure...
    assert!(
        w.journal
            .events
            .iter()
            .any(|e| matches!(e.event, Event::QuitUnpaid { business, .. } if business == bakery)),
        "payroll must have failed through the ordinary phase"
    );
    // ...the victims knew first-hand...
    assert!(firsthand_seen, "victims must have held direct beliefs");
    // ...and — the channel under guard — someone who never worked at the
    // bakery heard about it through gossip.
    assert!(
        stranger_heard,
        "gossip must carry the news to non-witnesses"
    );
    assert!(!w.is_halted());
}
