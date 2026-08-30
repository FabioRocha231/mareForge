use chrono::{DateTime, Utc};
use mareforge_shared::ids::{CharacterId, ItemDefinitionId, MarketOrderId, RegionId};
use serde::{Deserialize, Serialize};

use crate::currency::Money;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderStatus {
    #[default]
    Open,
    Partial,
    Filled,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketOrder {
    pub id: MarketOrderId,
    pub seller: CharacterId,
    pub item: ItemDefinitionId,
    pub quantity: u32,
    pub unit_price: Money,
    pub region: RegionId,
    pub status: OrderStatus,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// Quantidade já preenchida (para ordens parciais).
    pub filled_quantity: u32,
}

impl Default for MarketOrder {
    fn default() -> Self {
        Self {
            id: MarketOrderId::new(),
            seller: CharacterId::new(),
            item: ItemDefinitionId::new(),
            quantity: 0,
            unit_price: Money(0),
            region: RegionId::new(),
            status: OrderStatus::Open,
            created_at: Utc::now(),
            expires_at: Utc::now(),
            filled_quantity: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn market_order_defaults_to_open_with_zero_filled() {
        let order = MarketOrder::default();
        assert_eq!(order.status, OrderStatus::Open);
        assert_eq!(order.filled_quantity, 0);
    }
}
