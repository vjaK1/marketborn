//! Phase 4 close-out: the deficit lever end to end, and the remaining
//! policy commands through the queue.
//!
//! The fiscal cycle: kill the tax intake while the deficit lever is open
//! and the dole survives on borrowed money; restore the intake and the
//! surplus services the interest, retires the principal, and the bank
//! walks away with a sovereign margin. Every leg runs through commands
//! and full ticks (debug builds sweep every invariant every tick).

use sim_core::commands::PlayerCommand;
use sim_core::{Event, Money, World, WorldConfig};

const SEED: u64 = 42;

#[test]
fn the_deficit_cycle_borrows_for_the_dole_then_repays_the_bank() {
    let mut w = World::from_config(WorldConfig::default_with_seed(SEED));
    // Open the deficit lever, then kill the intake: the welfare state
    // runs on credit.
    w.queue_command(
        600,
        PlayerCommand::SetDeficitLimit {
            limit: Money::from_cents(50_000),
        },
    )
    .unwrap();
    w.queue_command(600, PlayerCommand::SetSalesTax { rate_bp: 0 })
        .unwrap();
    w.run_ticks(900).unwrap();

    assert!(
        w.state.government.debt > Money::ZERO,
        "the treasury must have borrowed for the dole"
    );
    assert!(w
        .journal
        .events
        .iter()
        .any(|e| matches!(e.event, Event::GovBorrowed { .. })));
    let dole_on_credit = w
        .journal
        .metrics
        .iter()
        .filter(|m| m.tick > 600 && m.tick <= 700)
        .map(|m| u64::from(m.welfare_recipients))
        .sum::<u64>();
    assert!(
        dole_on_credit > 0,
        "the dole must have survived the intake's death on borrowed money"
    );
    assert!(
        w.state.government.books.debt_capitalized > Money::ZERO,
        "with zero intake, sovereign interest must have compounded"
    );

    // Restore a strong intake AND suspend the dole — austerity. (Intake
    // alone cannot retire the debt: the credit era left a backlog of
    // destitution whose daily bill eats every cent of revenue before the
    // principal — an emergent poverty-debt trap. The dole-first priority
    // is by design; repayment therefore needs the floor lowered.)
    w.queue_command(901, PlayerCommand::SetSalesTax { rate_bp: 800 })
        .unwrap();
    w.queue_command(901, PlayerCommand::SetWelfareFloor { floor: Money::ZERO })
        .unwrap();
    w.run_ticks(600).unwrap();

    assert_eq!(
        w.state.government.debt,
        Money::ZERO,
        "the surplus must have retired the sovereign debt"
    );
    assert!(w
        .journal
        .events
        .iter()
        .any(|e| matches!(e.event, Event::GovDebtCleared)));
    assert!(
        w.state.bank.books.sovereign_interest > Money::ZERO
            || w.state.government.books.debt_capitalized > Money::ZERO,
        "the bank priced the state's credit"
    );
    assert_eq!(
        w.state.bank.books.sovereign_repaid + w.state.bank.books.sovereign_interest
            - w.state.bank.books.sovereign_disbursed,
        w.state.government.books.debt_interest_paid + w.state.government.books.debt_repaid
            - w.state.government.books.debt_drawn,
        "both sides of the sovereign ledger tell the same story"
    );
    assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    sim_core::invariants::check_all(&w.state, &w.journal).unwrap();
}

#[test]
fn the_minimum_wage_command_forces_town_wide_compliance_within_a_review_cycle() {
    let mut w = World::from_config(WorldConfig::default_with_seed(SEED));
    w.run_ticks(600).unwrap();
    assert!(
        w.state
            .businesses
            .values()
            .any(|b| b.wage < Money::from_cents(900)),
        "the scenario needs sub-$9 wages to force up"
    );
    w.queue_command(
        601,
        PlayerCommand::SetMinimumWage {
            wage: Money::from_cents(900),
        },
    )
    .unwrap();
    // Every business reviews within 7 ticks of the statute landing.
    w.run_ticks(10).unwrap();
    for b in w.state.businesses.values() {
        assert!(
            b.wage >= Money::from_cents(900),
            "{} still posts {} under a $9.00 statute",
            b.id,
            b.wage
        );
    }
}

#[test]
fn lever_commands_clamp_and_say_so() {
    let mut w = World::from_config(WorldConfig::default_with_seed(SEED));
    w.queue_command(
        1,
        PlayerCommand::SetWelfareFloor {
            floor: Money::from_cents(-500),
        },
    )
    .unwrap();
    w.queue_command(
        1,
        PlayerCommand::SetMinimumWage {
            wage: Money::from_cents(1),
        },
    )
    .unwrap();
    w.queue_command(
        1,
        PlayerCommand::SetDeficitLimit {
            limit: Money::from_cents(999_999_999),
        },
    )
    .unwrap();
    w.tick().unwrap();
    assert_eq!(w.state.government.welfare_floor, Money::ZERO);
    assert_eq!(
        w.state.government.minimum_wage,
        sim_core::government::MIN_MINIMUM_WAGE,
        "the statute cannot go below the mechanical wage floor"
    );
    assert_eq!(
        w.state.government.debt_limit,
        sim_core::government::MAX_DEFICIT_LIMIT
    );
    // A zero welfare floor is a legal policy: nobody qualifies, nothing
    // is paid.
    w.run_ticks(50).unwrap();
    assert_eq!(w.state.government.books.welfare_paid, Money::ZERO);
}
