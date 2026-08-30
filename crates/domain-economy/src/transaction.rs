use chrono::{DateTime, Utc};
use mareforge_shared::ids::{CharacterId, ItemDefinitionId, MarketOrderId, TransactionId};
use serde::{Deserialize, Serialize};

use crate::currency::Money;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub id: TransactionId,
    pub from: Option<CharacterId>, // None quando origem é NPC/mint
    pub to: Option<CharacterId>,   // None quando destino é NPC/sink
    pub order: MarketOrderId,
    pub item: ItemDefinitionId,
    pub quantity: u32,
    pub unit_price: Money,
    pub total: Money,
    pub executed_at: DateTime<Utc>,
}

impl Transaction {
    pub fn total_matches(&self) -> bool {
        self.total == Money(self.unit_price.0.saturating_mul(self.quantity as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transaction_with_total(total: u64) -> Transaction {
        Transaction {
            id: TransactionId::new(),
            from: None,
            to: None,
            order: MarketOrderId::new(),
            item: ItemDefinitionId::new(),
            quantity: 5,
            unit_price: Money(10),
            total: Money(total),
            executed_at: Utc::now(),
        }
    }

    #[test]
    fn total_matches_returns_true_for_correct_total() {
        assert!(transaction_with_total(50).total_matches());
    }

    #[test]
    fn total_matches_returns_false_for_wrong_total() {
        assert!(!transaction_with_total(49).total_matches());
    }
}
