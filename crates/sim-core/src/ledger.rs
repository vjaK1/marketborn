//! The single doorway for money movement.
//!
//! Every cent that moves does so through [`transfer`], [`mint`] or [`burn`]:
//! balances are checked, both sides are updated atomically (no fallible
//! operation between debit and credit), and a [`Transaction`] is journaled.
//! Nothing else in the codebase may touch `cash` fields directly.

use crate::goods::{Good, Qty};
use crate::ids::AccountId;
use crate::money::Money;
use crate::world::{Journal, SimState, MAX_TX_IN_MEMORY};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxKind {
    Wage,
    GoodsPurchase {
        good: Good,
        qty: Qty,
        unit_price: Money,
    },
    Dividend,
    /// Owner recapitalizes their own business from personal savings.
    OwnerInvestment,
    /// Explicit money creation/destruction via player command.
    MonetaryPolicy {
        memo: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub seq: u64,
    pub tick: u64,
    /// `None` = minted by the monetary authority.
    pub from: Option<AccountId>,
    /// `None` = withdrawn by the monetary authority.
    pub to: Option<AccountId>,
    pub amount: Money,
    pub kind: TxKind,
}

impl Transaction {
    pub fn touches(&self, account: AccountId) -> bool {
        self.from == Some(account) || self.to == Some(account)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    #[error("unknown account {0}")]
    UnknownAccount(AccountId),
    #[error("insufficient funds in {account}: has {available}, needs {required}")]
    InsufficientFunds {
        account: AccountId,
        available: Money,
        required: Money,
    },
    #[error("transfer amount {0} is negative")]
    NegativeAmount(Money),
    #[error("transfer from an account to itself: {0}")]
    SelfTransfer(AccountId),
}

pub fn balance(state: &SimState, account: AccountId) -> Result<Money, LedgerError> {
    match account {
        AccountId::Agent(id) => state
            .agents
            .get(&id)
            .map(|a| a.cash)
            .ok_or(LedgerError::UnknownAccount(account)),
        AccountId::Business(id) => state
            .businesses
            .get(&id)
            .map(|b| b.cash)
            .ok_or(LedgerError::UnknownAccount(account)),
    }
}

fn cash_mut(state: &mut SimState, account: AccountId) -> Result<&mut Money, LedgerError> {
    match account {
        AccountId::Agent(id) => state
            .agents
            .get_mut(&id)
            .map(|a| &mut a.cash)
            .ok_or(LedgerError::UnknownAccount(account)),
        AccountId::Business(id) => state
            .businesses
            .get_mut(&id)
            .map(|b| &mut b.cash)
            .ok_or(LedgerError::UnknownAccount(account)),
    }
}

fn record(journal: &mut Journal, tx: Transaction) {
    journal.transactions.push_back(tx);
    if journal.transactions.len() > MAX_TX_IN_MEMORY {
        journal.transactions.pop_front();
    }
    journal.next_tx_seq += 1;
}

/// Move `amount` between two accounts. Zero-amount transfers are recorded
/// as no-ops without a journal entry.
pub fn transfer(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    from: AccountId,
    to: AccountId,
    amount: Money,
    kind: TxKind,
) -> Result<(), LedgerError> {
    if amount.is_negative() {
        return Err(LedgerError::NegativeAmount(amount));
    }
    if from == to {
        return Err(LedgerError::SelfTransfer(from));
    }
    if amount == Money::ZERO {
        return Ok(());
    }
    // Validate both sides before mutating anything.
    let available = balance(state, from)?;
    balance(state, to)?;
    if available < amount {
        return Err(LedgerError::InsufficientFunds {
            account: from,
            available,
            required: amount,
        });
    }
    // Infallible from here on: both accounts exist and funds suffice.
    #[allow(clippy::unwrap_used)] // proven Ok by the balance checks above
    {
        *cash_mut(state, from).unwrap() -= amount;
        *cash_mut(state, to).unwrap() += amount;
    }
    record(
        journal,
        Transaction {
            seq: journal.next_tx_seq,
            tick,
            from: Some(from),
            to: Some(to),
            amount,
            kind,
        },
    );
    Ok(())
}

/// Create money at an account (explicit monetary policy only). Adjusts the
/// conservation invariant's expected total.
pub fn mint(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    to: AccountId,
    amount: Money,
    memo: String,
) -> Result<(), LedgerError> {
    if amount.is_negative() {
        return Err(LedgerError::NegativeAmount(amount));
    }
    *cash_mut(state, to)? += amount;
    state.expected_total_money += amount;
    if let AccountId::Business(id) = to {
        if let Some(b) = state.businesses.get_mut(&id) {
            b.books.policy_net += amount;
        }
    }
    record(
        journal,
        Transaction {
            seq: journal.next_tx_seq,
            tick,
            from: None,
            to: Some(to),
            amount,
            kind: TxKind::MonetaryPolicy { memo },
        },
    );
    Ok(())
}

/// Destroy money at an account (explicit monetary policy only).
pub fn burn(
    state: &mut SimState,
    journal: &mut Journal,
    tick: u64,
    from: AccountId,
    amount: Money,
    memo: String,
) -> Result<(), LedgerError> {
    if amount.is_negative() {
        return Err(LedgerError::NegativeAmount(amount));
    }
    let available = balance(state, from)?;
    if available < amount {
        return Err(LedgerError::InsufficientFunds {
            account: from,
            available,
            required: amount,
        });
    }
    #[allow(clippy::unwrap_used)] // proven Ok by the balance check above
    {
        *cash_mut(state, from).unwrap() -= amount;
    }
    state.expected_total_money -= amount;
    if let AccountId::Business(id) = from {
        if let Some(b) = state.businesses.get_mut(&id) {
            b.books.policy_net -= amount;
        }
    }
    record(
        journal,
        Transaction {
            seq: journal.next_tx_seq,
            tick,
            from: Some(from),
            to: None,
            amount,
            kind: TxKind::MonetaryPolicy { memo },
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldConfig;
    use crate::World;

    fn world() -> World {
        World::from_config(WorldConfig::default_with_seed(7))
    }

    fn first_agent(w: &World) -> AccountId {
        AccountId::Agent(*w.state.agents.keys().next().unwrap())
    }

    fn first_business(w: &World) -> AccountId {
        AccountId::Business(*w.state.businesses.keys().next().unwrap())
    }

    #[test]
    fn transfer_moves_money_and_journals() {
        let mut w = world();
        let a = first_agent(&w);
        let b = first_business(&w);
        let before_total = w.state.total_cash();
        let a_before = balance(&w.state, a).unwrap();
        let b_before = balance(&w.state, b).unwrap();
        transfer(
            &mut w.state,
            &mut w.journal,
            1,
            a,
            b,
            Money::from_cents(500),
            TxKind::Wage,
        )
        .unwrap();
        assert_eq!(
            balance(&w.state, a).unwrap(),
            a_before - Money::from_cents(500)
        );
        assert_eq!(
            balance(&w.state, b).unwrap(),
            b_before + Money::from_cents(500)
        );
        assert_eq!(w.state.total_cash(), before_total, "conservation");
        assert_eq!(w.journal.transactions.len(), 1);
    }

    #[test]
    fn transfer_rejects_insufficient_funds_without_mutation() {
        let mut w = world();
        let a = first_agent(&w);
        let b = first_business(&w);
        let a_before = balance(&w.state, a).unwrap();
        let err = transfer(
            &mut w.state,
            &mut w.journal,
            1,
            a,
            b,
            a_before + Money::from_cents(1),
            TxKind::Wage,
        )
        .unwrap_err();
        assert!(matches!(err, LedgerError::InsufficientFunds { .. }));
        assert_eq!(balance(&w.state, a).unwrap(), a_before);
        assert!(w.journal.transactions.is_empty());
    }

    #[test]
    fn transfer_rejects_negative_and_self() {
        let mut w = world();
        let a = first_agent(&w);
        let b = first_business(&w);
        assert!(matches!(
            transfer(
                &mut w.state,
                &mut w.journal,
                1,
                a,
                b,
                Money::from_cents(-1),
                TxKind::Wage
            ),
            Err(LedgerError::NegativeAmount(_))
        ));
        assert!(matches!(
            transfer(
                &mut w.state,
                &mut w.journal,
                1,
                a,
                a,
                Money::from_cents(1),
                TxKind::Wage
            ),
            Err(LedgerError::SelfTransfer(_))
        ));
    }

    #[test]
    fn mint_and_burn_adjust_expected_total() {
        let mut w = world();
        let a = first_agent(&w);
        let expected_before = w.state.expected_total_money;
        mint(
            &mut w.state,
            &mut w.journal,
            1,
            a,
            Money::from_cents(1000),
            "stimulus".into(),
        )
        .unwrap();
        assert_eq!(
            w.state.expected_total_money,
            expected_before + Money::from_cents(1000)
        );
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
        burn(
            &mut w.state,
            &mut w.journal,
            1,
            a,
            Money::from_cents(400),
            "tax burn".into(),
        )
        .unwrap();
        assert_eq!(
            w.state.expected_total_money,
            expected_before + Money::from_cents(600)
        );
        assert_eq!(w.state.total_cash(), w.state.expected_total_money);
    }
}
