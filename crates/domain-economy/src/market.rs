//! Sink de moeda e regras de preço (PRD §46/§47, MF-026). Listing fee e
//! transaction tax são os dois primeiros sinks do jogo: ouro sai da
//! circulação de verdade. Os bps são tuning inicial — configuração, não
//! código espalhado.

use serde::{Deserialize, Serialize};

use crate::currency::Money;

/// Taxas do mercado em pontos-base (1 bps = 0,01%).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeePolicy {
    /// Cobrada ao criar a order sobre o valor total anunciado; não
    /// reembolsável (§46).
    pub listing_fee_bps: u64,
    /// Descontada dos proceeds do seller na execução (§47).
    pub transaction_tax_bps: u64,
    /// Janela de vida de uma order nova (MF-041).
    pub default_order_duration_secs: u64,
}

impl Default for FeePolicy {
    fn default() -> Self {
        Self {
            listing_fee_bps: 100,             // 1%
            transaction_tax_bps: 300,         // 3%
            default_order_duration_secs: 300, // 5 min (MF-041)
        }
    }
}

impl FeePolicy {
    fn fee_of(&self, total: Money, bps: u64) -> Money {
        // Arredonda pra cima: sink nunca arredonda a favor do jogador.
        // Matemática em u128: total*bps não pode estourar no caminho.
        let raw = (u128::from(total.0) * u128::from(bps)).div_ceil(10_000);
        Money(u64::try_from(raw).unwrap_or(u64::MAX))
    }

    /// Valor total anunciado/executado (preço unitário × quantidade).
    pub fn total(&self, unit_price: Money, quantity: u32) -> Money {
        Money(unit_price.0.saturating_mul(u64::from(quantity)))
    }

    /// Listing fee sobre o valor anunciado (§46).
    pub fn listing_fee(&self, unit_price: Money, quantity: u32) -> Money {
        self.fee_of(self.total(unit_price, quantity), self.listing_fee_bps)
    }

    /// O que o seller recebe: total menos a transaction tax (§47).
    pub fn net_proceeds(&self, total: Money) -> Money {
        let tax = self.fee_of(total, self.transaction_tax_bps);
        Money(total.0.saturating_sub(tax.0))
    }

    /// Taxa de transação sobre o valor executado (para o ledger de burn).
    pub fn transaction_tax(&self, total: Money) -> Money {
        self.fee_of(total, self.transaction_tax_bps)
    }
}

/// Erros de mercado — fail-closed (§69: UnknownMarket e afins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MarketError {
    #[error("preço de order precisa ser maior que zero")]
    ZeroPrice,
    #[error("quantidade de order precisa ser maior que zero")]
    ZeroQuantity,
    #[error("saldo insuficiente: precisa de {needed} mas tem {available}")]
    InsufficientFunds { needed: Money, available: Money },
    #[error("order desconhecida (UnknownOrder)")]
    UnknownOrder,
    #[error("order não está aberta (NotOpen)")]
    OrderNotOpen,
    #[error("order de outra região (RegionMismatch) — mercado não é global")]
    RegionMismatch,
    #[error("quantidade insuficiente na order: pediu {requested}, tem {available}")]
    InsufficientQuantity { requested: u32, available: u32 },
    #[error("item não está no storage local")]
    NotInStorage,
    #[error("storage vazio ou inexistente nesta região")]
    EmptyStorage,
    #[error("essa order não pertence a este personagem")]
    NotOrderOwner,
}

/// Validação de nova order (preço e quantidade positivos).
pub fn validate_new_order(unit_price: Money, quantity: u32) -> Result<(), MarketError> {
    if unit_price.0 == 0 {
        return Err(MarketError::ZeroPrice);
    }
    if quantity == 0 {
        return Err(MarketError::ZeroQuantity);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_matches_prd_bps() {
        let policy = FeePolicy::default();
        assert_eq!(policy.listing_fee_bps, 100); // §46: 1%
        assert_eq!(policy.transaction_tax_bps, 300); // §47: 3%
        assert_eq!(policy.default_order_duration_secs, 300); // MF-041
    }

    #[test]
    fn listing_fee_is_one_percent_of_announced_value() {
        let policy = FeePolicy::default();
        // 5g × 100 unidades = 500g anunciados; 1% = 5g.
        assert_eq!(policy.listing_fee(Money(5), 100), Money(5));
    }

    #[test]
    fn fee_rounds_up_never_in_favor_of_player() {
        let policy = FeePolicy::default();
        // 3 de anúncio × 1% = 0,03 → piso seria 0; teto dá 1: sink é sink.
        assert_eq!(policy.listing_fee(Money(3), 1), Money(1));
    }

    #[test]
    fn net_proceeds_deducts_transaction_tax() {
        let policy = FeePolicy::default();
        let total = policy.total(Money(10), 50); // 500g
        assert_eq!(policy.transaction_tax(total), Money(15));
        assert_eq!(policy.net_proceeds(total), Money(485));
    }

    #[test]
    fn total_uses_saturating_math() {
        let policy = FeePolicy::default();
        let huge = Money(u64::MAX);
        assert_eq!(policy.total(huge, u32::MAX), Money(u64::MAX));
    }

    #[test]
    fn new_order_needs_positive_price_and_quantity() {
        assert_eq!(validate_new_order(Money(5), 10), Ok(()));
        assert_eq!(
            validate_new_order(Money(0), 10),
            Err(MarketError::ZeroPrice)
        );
        assert_eq!(
            validate_new_order(Money(5), 0),
            Err(MarketError::ZeroQuantity)
        );
    }
}
