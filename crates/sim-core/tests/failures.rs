//! Phase 6 failure tests (the BRIEF's list): stage each catastrophe and
//! assert the one hardening guarantee — the world DEGRADES WITHOUT
//! HALTING. Full ticks in debug builds sweep all nine invariants every
//! day, so "ran N ticks" already means "stayed reconciled through the
//! disaster"; each test adds the scenario's own liveness/degradation
//! evidence on top.
//!
//! Corrupted saves and newer schema versions are covered where they
//! belong, in sim-persist's tests (garbage files and version refusal,
//! since Phase 0).
//!
//! Staging rules (the invariants police them): raw cash surgery must
//! resync books carrying `taxes_paid` forward (the remittance sum is
//! global — DECISIONS #029); raw inventory surgery must resync the
//! per-good conservation targets.

use sim_core::commands::PlayerCommand;
use sim_core::goods::Good;
use sim_core::ids::AccountId;
use sim_core::money::Money;
use sim_core::worldgen::WorldConfig;
use sim_core::{Books, Event, World};

/// Zero a business's cash keeping every identity green.
fn bankrupt(w: &mut World, id: sim_core::BusinessId) {
    let b = w.state.businesses.get_mut(&id).unwrap();
    b.cash = Money::ZERO;
    let taxes = b.books.taxes_paid;
    b.books = Books::new(taxes);
    b.books.taxes_paid = taxes;
}

/// Recompute the goods conservation targets after inventory surgery.
fn resync_goods(w: &mut World) {
    for good in Good::ALL {
        let total = w.state.total_goods(good);
        w.state.expected_total_goods.insert(good, total);
    }
}

fn assert_alive(w: &World) {
    assert!(!w.is_halted(), "the world must degrade, not halt");
    assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    sim_core::invariants::check_all(&w.state, &w.journal).unwrap();
}

#[test]
fn empty_markets_starve_but_do_not_halt() {
    let mut w = World::from_config(WorldConfig::default_with_seed(42));
    // Every shelf and every pantry: bare.
    for b in w.state.businesses.values_mut() {
        b.inventory.clear();
    }
    for a in w.state.agents.values_mut() {
        a.pantry = 0;
    }
    resync_goods(&mut w);
    w.run_ticks(120).unwrap();
    assert_alive(&w);
    // Hunger struck immediately — and production restarted the town.
    assert!(w
        .journal
        .events
        .iter()
        .any(|e| matches!(e.event, Event::AgentHungry { .. })));
    let m = w.journal.metrics.back().unwrap();
    assert!(
        m.food_produced > 0,
        "production must restart from bare shelves"
    );
}

#[test]
fn a_town_with_no_employers_reaches_for_the_dole() {
    let mut w = World::from_config(WorldConfig::default_with_seed(42));
    let ids: Vec<_> = w.state.businesses.keys().copied().collect();
    for id in &ids {
        let workers = w.state.businesses.get_mut(id).unwrap().workers.clone();
        for aid in workers {
            w.state.agents.get_mut(&aid).unwrap().employer = None;
        }
        w.state.businesses.get_mut(id).unwrap().workers.clear();
        bankrupt(&mut w, *id);
    }
    // Owners too broke to inject: the whole private economy is dead.
    for a in w.state.agents.values_mut() {
        a.cash = Money::from_cents(500);
    }
    w.state.expected_total_money = w.state.total_cash();
    w.run_ticks(200).unwrap();
    assert_alive(&w);
    let m = w.journal.metrics.back().unwrap();
    assert!(m.hungry > 0, "a dead economy is a hungry one");
    // The welfare floor still catches the destitute while any treasury
    // trickle lasts; with no trade there is no tax, so the dole is
    // bounded — the point is the MACHINERY survives, not that it saves
    // everyone.
    assert!(w.state.government.cash >= Money::ZERO);
}

#[test]
fn mass_bankruptcy_fires_everyone_cleanly() {
    let mut w = World::from_config(WorldConfig::default_with_seed(42));
    let ids: Vec<_> = w.state.businesses.keys().copied().collect();
    for id in &ids {
        bankrupt(&mut w, *id);
    }
    w.state.expected_total_money = w.state.total_cash();
    w.run_ticks(150).unwrap();
    assert_alive(&w);
    assert!(
        w.journal
            .events
            .iter()
            .any(|e| matches!(e.event, Event::QuitUnpaid { .. })),
        "missed payrolls must shed workers through the ordinary machinery"
    );
}

#[test]
fn an_insolvent_bank_stops_lending_and_the_world_keeps_turning() {
    let mut w = World::from_config(WorldConfig::default_with_seed(42));
    let borrower = *w.state.businesses.keys().next().unwrap();
    sim_core::bank::issue(
        &mut w.state,
        &mut w.journal,
        0,
        borrower,
        Money::from_cents(20_000),
    )
    .unwrap();
    // Drain what's left of the bank: insolvent mid-loan.
    let remaining = w.state.bank.cash;
    sim_core::ledger::burn(
        &mut w.state,
        &mut w.journal,
        0,
        AccountId::Bank,
        remaining,
        "bank run".into(),
    )
    .unwrap();
    assert_eq!(w.state.bank.cash, Money::ZERO);
    // New credit is refused; the deficit lever finds nothing lendable.
    let other = *w.state.businesses.keys().nth(1).unwrap();
    assert_eq!(
        sim_core::bank::assess(&w.state, other, Money::from_cents(1_000)),
        Err(sim_core::bank::CreditRefusal::BankIlliquid)
    );
    w.queue_command(
        1,
        PlayerCommand::SetDeficitLimit {
            limit: Money::from_cents(50_000),
        },
    )
    .unwrap();
    w.run_ticks(150).unwrap();
    assert_alive(&w);
    // The existing loan still serviced (or missed) through the ordinary
    // machinery — the bank collecting while broke is fine; lending isn't.
    assert!(w.state.bank.books.interest_received >= Money::ZERO);
}

#[test]
fn resource_exhaustion_no_farms_is_famine_not_a_crash() {
    let mut w = World::from_config(WorldConfig::default_with_seed(42));
    let farm_ids: Vec<_> = w
        .state
        .businesses
        .values()
        .filter(|b| b.sells == Good::Wheat)
        .map(|b| b.id)
        .collect();
    for id in &farm_ids {
        let workers = w.state.businesses.get_mut(id).unwrap().workers.clone();
        for aid in workers {
            w.state.agents.get_mut(&aid).unwrap().employer = None;
        }
        let b = w.state.businesses.get_mut(id).unwrap();
        b.workers.clear();
        b.target_headcount = 0;
        b.inventory.clear();
    }
    resync_goods(&mut w);
    w.run_ticks(200).unwrap();
    assert_alive(&w);
    let m = w.journal.metrics.back().unwrap();
    assert!(
        m.hungry >= 20,
        "no agriculture must mean deep hunger (saw {} hungry)",
        m.hungry
    );
}

#[test]
fn extreme_inflation_ratchets_prices_without_breaking_anything() {
    let mut w = World::from_config(WorldConfig::default_with_seed(42));
    let food_before = {
        w.run_ticks(30).unwrap();
        w.state
            .market
            .last_prices
            .get(&Good::Food)
            .copied()
            .unwrap_or(Money::from_cents(540))
    };
    // ~×100 the money supply, straight into every pocket.
    let ids: Vec<_> = w.state.agents.keys().copied().collect();
    for (i, aid) in ids.iter().enumerate() {
        w.queue_command(
            31 + (i as u64 % 3),
            PlayerCommand::AdjustMoneySupply {
                account: AccountId::Agent(*aid),
                delta: Money::from_cents(7_500_000),
                memo: "helicopter".into(),
            },
        )
        .unwrap();
    }
    w.run_ticks(270).unwrap();
    assert_alive(&w);
    let food_after = w
        .state
        .market
        .last_prices
        .get(&Good::Food)
        .copied()
        .unwrap_or(Money::ZERO);
    assert!(
        food_after > food_before + food_before,
        "a 100x money supply must at least double food ({food_before} -> {food_after})"
    );
}

#[test]
fn a_thousand_agent_world_generates_sane_and_ticks_green() {
    let mut w = World::from_config(WorldConfig {
        master_seed: 42,
        population: 1_000,
        hash_every: 50,
    });
    assert_eq!(w.state.agents.len(), 1_000);
    // Scaling rule: one instance per N agents, ceiling, per template
    // (farms /15 + mills /35 + bakeries /30 + six specialists /100).
    let expected_businesses = 1_000u32.div_ceil(15)
        + 1_000u32.div_ceil(35)
        + 1_000u32.div_ceil(30)
        + 6 * 1_000u32.div_ceil(100);
    assert_eq!(w.state.businesses.len() as u32, expected_businesses);
    w.run_ticks(100).unwrap();
    assert_alive(&w);
    let m = w.journal.metrics.back().unwrap();
    assert!(m.employed > 0 && m.food_produced > 0);
}

/// The year-long pop-1000 run (check:full tier): the scaling question's
/// measurement lives in PROGRESS.md — this guards "no halt, no panic".
#[test]
#[ignore = "slow pop-1000 soak; part of check:full"]
fn a_thousand_agent_world_survives_a_year() {
    let mut w = World::from_config(WorldConfig {
        master_seed: 42,
        population: 1_000,
        hash_every: 50,
    });
    w.run_ticks(365).unwrap();
    assert_alive(&w);
    let m = w.journal.metrics.back().unwrap();
    assert!(m.food_produced > 0, "the megatown still bakes at year one");
}
