//! Money as `i64` minor units (cents). No floating point, ever.
//!
//! Rates are integer basis points (1 bp = 0.01%). Division rounds toward
//! zero; wherever a computation produces a remainder, the caller assigns it
//! explicitly to a party (see `docs/ECONOMIC_RULES.md` §Money).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

/// An amount of money in cents. May be negative in arithmetic (deltas),
/// but account balances are invariant-checked to stay non-negative.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub struct Money(i64);

impl Money {
    pub const ZERO: Money = Money(0);
    pub const MAX: Money = Money(i64::MAX);

    pub const fn from_cents(cents: i64) -> Money {
        Money(cents)
    }

    pub const fn cents(self) -> i64 {
        self.0
    }

    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// `self * bp / 10_000`, rounding toward zero, computed in i128 so it
    /// cannot overflow for any realistic ledger amount.
    pub fn mul_bp(self, bp: i64) -> Money {
        let v = (self.0 as i128 * bp as i128) / 10_000;
        Money(v as i64)
    }

    /// Multiply by an integer quantity, erroring on overflow.
    pub fn checked_mul_qty(self, qty: i64) -> Option<Money> {
        self.0.checked_mul(qty).map(Money)
    }

    /// Integer division of self by a positive unit price: how many whole
    /// units are affordable. Rounds toward zero.
    pub fn affordable_units(self, unit_price: Money) -> i64 {
        if unit_price.0 <= 0 {
            return i64::MAX;
        }
        (self.0 / unit_price.0).max(0)
    }

    pub fn min(self, other: Money) -> Money {
        Money(self.0.min(other.0))
    }

    pub fn max(self, other: Money) -> Money {
        Money(self.0.max(other.0))
    }
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money(self.0 + rhs.0)
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Money) {
        self.0 += rhs.0;
    }
}

impl Sub for Money {
    type Output = Money;
    fn sub(self, rhs: Money) -> Money {
        Money(self.0 - rhs.0)
    }
}

impl SubAssign for Money {
    fn sub_assign(&mut self, rhs: Money) {
        self.0 -= rhs.0;
    }
}

impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money(-self.0)
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        Money(iter.map(|m| m.0).sum())
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        write!(f, "{sign}${}.{:02}", abs / 100, abs % 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_bp_rounds_toward_zero() {
        assert_eq!(Money::from_cents(999).mul_bp(2500), Money::from_cents(249)); // 249.75 -> 249
        assert_eq!(
            Money::from_cents(-999).mul_bp(2500),
            Money::from_cents(-249) // -249.75 -> -249 (toward zero)
        );
        assert_eq!(
            Money::from_cents(100).mul_bp(10_000),
            Money::from_cents(100)
        );
        assert_eq!(Money::from_cents(1).mul_bp(9_999), Money::ZERO);
    }

    #[test]
    fn mul_bp_survives_large_amounts() {
        let big = Money::from_cents(i64::MAX / 2);
        // Would overflow i64 without the i128 intermediate.
        assert_eq!(big.mul_bp(10_000), big);
    }

    #[test]
    fn affordable_units_rounds_down() {
        let cash = Money::from_cents(1000);
        assert_eq!(cash.affordable_units(Money::from_cents(300)), 3);
        assert_eq!(cash.affordable_units(Money::from_cents(1001)), 0);
        assert_eq!(
            Money::from_cents(-5).affordable_units(Money::from_cents(1)),
            0
        );
    }

    #[test]
    fn display_format() {
        assert_eq!(Money::from_cents(123456).to_string(), "$1234.56");
        assert_eq!(Money::from_cents(-5).to_string(), "-$0.05");
        assert_eq!(Money::ZERO.to_string(), "$0.00");
    }
}
