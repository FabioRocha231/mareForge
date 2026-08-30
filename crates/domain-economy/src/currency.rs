use serde::{Deserialize, Serialize};
use std::fmt;

/// Identifica uma moeda. Apenas "Gold" por enquanto; estrutura permite múltiplas moedas.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Currency {
    pub id: String,     // ex.: "gold"
    pub name: String,   // ex.: "Gold"
    pub symbol: String, // ex.: "g"
}

impl Currency {
    pub fn gold() -> Currency {
        Currency {
            id: "gold".into(),
            name: "Gold".into(),
            symbol: "g".into(),
        }
    }
}

/// Quantia em unidades mínimas (centavos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money(pub u64);

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for Money {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<u32> for Money {
    fn from(value: u32) -> Self {
        Self(value as u64)
    }
}

impl Money {
    pub fn saturating_add(self, other: Money) -> Money {
        Money(self.0.saturating_add(other.0))
    }

    pub fn saturating_sub(self, other: Money) -> Money {
        Money(self.0.saturating_sub(other.0))
    }

    pub fn checked_mul(self, rhs: u64) -> Option<Money> {
        self.0.checked_mul(rhs).map(Money)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gold_returns_expected_currency() {
        assert_eq!(
            Currency::gold(),
            Currency {
                id: "gold".into(),
                name: "Gold".into(),
                symbol: "g".into(),
            }
        );
    }

    #[test]
    fn money_arithmetic_saturates_and_checks_overflow() {
        assert_eq!(Money(100).saturating_add(Money(50)), Money(150));
        assert_eq!(Money::from(0u64).saturating_sub(Money(10)), Money(0));
        assert_eq!(Money(10).checked_mul(5), Some(Money(50)));
        assert_eq!(Money(u64::MAX).checked_mul(2), None);
    }

    #[test]
    fn money_converts_from_u32() {
        let money: Money = 100u32.into();
        assert_eq!(money, Money(100));
    }
}
