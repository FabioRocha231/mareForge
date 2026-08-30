//! VWAP regional por item (MF-051). Preço não é propriedade do item; emerge
//! do mercado. Cada trade executado atualiza uma janela FIFO das últimas N
//! unidades negociadas por `(region, item)`. Não persistido: alpha aceita
//! reset entre restarts.

use std::collections::{HashMap, VecDeque};

use mareforge_shared::ids::{ItemDefinitionId, RegionId};

use crate::currency::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PriceEntry {
    unit_price: Money,
    quantity: u32,
}

#[derive(Debug, Default)]
struct Window {
    entries: VecDeque<PriceEntry>,
    total_quantity: u32,
}

/// VWAP de uma janela de trades `(region, item)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vwap {
    /// Preço unitário médio ponderado por volume, em centavos de ouro.
    pub unit_price: Money,
    /// Quantidade total coberta pela janela atual (sempre <= `window_size`).
    pub sample_quantity: u32,
    /// Quantos trades estão dentro da janela — útil para diagnóstico
    /// ("VWAP com 1 trade" vs "VWAP com 50 trades").
    pub trade_count: usize,
}

/// Index de preço por mercado regional. Janela rolante em unidades (não em
/// trades): a soma das `quantity` dos entries da janela não excede
/// `window_size`. Eviction é por unidade (FIFO preciso): se um trade for
/// maior que a janela, ele é dividido e a parte excedente não conta.
#[derive(Debug)]
pub struct MarketPriceIndex {
    window_size: u32,
    state: HashMap<(RegionId, ItemDefinitionId), Window>,
}

impl MarketPriceIndex {
    /// Janela padrão do alpha: 100 unidades (MF-051).
    pub const DEFAULT_WINDOW_SIZE: u32 = 100;

    pub fn new(window_size: u32) -> Self {
        Self {
            window_size,
            state: HashMap::new(),
        }
    }

    pub fn window_size(&self) -> u32 {
        self.window_size
    }

    /// Registra um trade executado. `region` é parte do contrato do evento
    /// (fail-closed pelo tipo — `RegionId` é exigido, não derivado depois).
    pub fn record_trade(
        &mut self,
        region: RegionId,
        item: ItemDefinitionId,
        unit_price: Money,
        quantity: u32,
    ) {
        if quantity == 0 {
            // Trade de quantidade zero não move o index. Sem panic, sem
            // janela vazia "contando" — alpha aceita silenciar.
            return;
        }
        let window = self.state.entry((region, item)).or_default();
        // Trunca o trade para caber na janela inteira. Se for maior que
        // `window_size`, só a parte inicial conta; o resto é descartado.
        // Caso normal: a janela cabe inteira.
        let admitted = quantity.min(self.window_size);
        window.entries.push_back(PriceEntry {
            unit_price,
            quantity: admitted,
        });
        window.total_quantity = window.total_quantity.saturating_add(admitted);
        // Se a janela anterior + o trade admitido ainda exceder, descarta
        // do mais antigo até caber. Janela nunca excede `window_size`.
        let excess = window.total_quantity.saturating_sub(self.window_size);
        if excess > 0 {
            Self::evict_units(&mut window.entries, &mut window.total_quantity, excess);
        }
    }

    /// Remove `to_remove` unidades da frente do deque, dividindo entries
    /// quando a eviction é parcial. Atualiza `total_quantity` no caminho.
    fn evict_units(
        entries: &mut VecDeque<PriceEntry>,
        total_quantity: &mut u32,
        mut to_remove: u32,
    ) {
        while to_remove > 0 {
            match entries.front_mut() {
                Some(front) => {
                    if front.quantity <= to_remove {
                        to_remove -= front.quantity;
                        *total_quantity -= front.quantity;
                        entries.pop_front();
                    } else {
                        // Partial: o entry sobrevive com `quantity - to_remove`.
                        *total_quantity -= to_remove;
                        front.quantity -= to_remove;
                        to_remove = 0;
                    }
                }
                None => break,
            }
        }
    }

    /// VWAP atual para `(region, item)`. `None` se nenhum trade foi
    /// registrado nessa chave — caller decide o que fazer com "sem preço"
    /// (MF-052: nunca tratar como zero).
    pub fn vwap(&self, region: RegionId, item: ItemDefinitionId) -> Option<Vwap> {
        let window = self.state.get(&(region, item))?;
        if window.total_quantity == 0 {
            return None;
        }
        let mut weighted: u128 = 0;
        for entry in &window.entries {
            weighted = weighted.saturating_add(
                u128::from(entry.unit_price.0).saturating_mul(u128::from(entry.quantity)),
            );
        }
        let total = u128::from(window.total_quantity);
        let vwap_units = u64::try_from(weighted / total).unwrap_or(u64::MAX);
        Some(Vwap {
            unit_price: Money(vwap_units),
            sample_quantity: window.total_quantity,
            trade_count: window.entries.len(),
        })
    }

    /// Quantas chaves `(region, item)` distintas têm pelo menos um trade
    /// registrado. Para telemetria / debug.
    pub fn tracked_markets(&self) -> usize {
        self.state.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region() -> RegionId {
        RegionId::new()
    }

    fn item() -> ItemDefinitionId {
        ItemDefinitionId::new()
    }

    #[test]
    fn single_trade_vwap_equals_trade_price() {
        let mut index = MarketPriceIndex::new(100);
        let r = region();
        let it = item();
        index.record_trade(r, it, Money(10), 5);
        let vwap = index.vwap(r, it).expect("há um trade");
        assert_eq!(vwap.unit_price, Money(10));
        assert_eq!(vwap.sample_quantity, 5);
        assert_eq!(vwap.trade_count, 1);
    }

    #[test]
    fn multiple_trades_weighted_by_quantity() {
        let mut index = MarketPriceIndex::new(100);
        let r = region();
        let it = item();
        // 30 unidades a 10g + 50 unidades a 12g = (300 + 600) / 80 = 11g.
        index.record_trade(r, it, Money(10), 30);
        index.record_trade(r, it, Money(12), 50);
        let vwap = index.vwap(r, it).expect("há dois trades");
        assert_eq!(vwap.unit_price, Money(11));
        assert_eq!(vwap.sample_quantity, 80);
        assert_eq!(vwap.trade_count, 2);
    }

    #[test]
    fn fifo_window_drops_oldest_when_exceeding_size() {
        let mut index = MarketPriceIndex::new(100);
        let r = region();
        let it = item();
        // 80 unidades a 10g → cabe.
        index.record_trade(r, it, Money(10), 80);
        // 50 unidades a 20g → 130 > 100; descarta 30 unidades do trade 1
        // (entrada mais antiga). Sobram 50 unidades a 10g + 50 a 20g.
        // VWAP = (50*10 + 50*20) / 100 = 15g.
        index.record_trade(r, it, Money(20), 50);
        let vwap = index.vwap(r, it).expect("há trades");
        assert_eq!(vwap.unit_price, Money(15));
        assert_eq!(vwap.sample_quantity, 100);
        assert_eq!(vwap.trade_count, 2);
    }

    #[test]
    fn single_trade_larger_than_window_is_clipped() {
        let mut index = MarketPriceIndex::new(50);
        let r = region();
        let it = item();
        // Trade único de 100 unidades em janela de 50 — só a primeira
        // metade entra; o resto é descartado.
        index.record_trade(r, it, Money(7), 100);
        let vwap = index.vwap(r, it).expect("metade do trade entrou");
        assert_eq!(vwap.unit_price, Money(7));
        assert_eq!(vwap.sample_quantity, 50);
        assert_eq!(vwap.trade_count, 1);
        assert_eq!(index.tracked_markets(), 1);
    }

    #[test]
    fn zero_quantity_trade_is_noop() {
        let mut index = MarketPriceIndex::new(10);
        let r = region();
        let it = item();
        index.record_trade(r, it, Money(99), 0);
        assert!(index.vwap(r, it).is_none());
        assert_eq!(index.tracked_markets(), 0);
    }

    #[test]
    fn markets_isolated_per_region() {
        let mut index = MarketPriceIndex::new(100);
        let r1 = region();
        let r2 = region();
        let it = item();
        index.record_trade(r1, it, Money(10), 10);
        index.record_trade(r2, it, Money(99), 10);
        let v1 = index.vwap(r1, it).expect("região 1 tem trade");
        let v2 = index.vwap(r2, it).expect("região 2 tem trade");
        assert_eq!(v1.unit_price, Money(10));
        assert_eq!(v2.unit_price, Money(99));
        // Mesma região não mistura.
        assert_ne!(v1.unit_price, v2.unit_price);
    }

    #[test]
    fn markets_isolated_per_item() {
        let mut index = MarketPriceIndex::new(100);
        let r = region();
        let a = item();
        let b = item();
        index.record_trade(r, a, Money(5), 10);
        index.record_trade(r, b, Money(50), 10);
        assert_eq!(index.vwap(r, a).unwrap().unit_price, Money(5));
        assert_eq!(index.vwap(r, b).unwrap().unit_price, Money(50));
    }

    #[test]
    fn vwap_none_when_no_trade_recorded() {
        let index = MarketPriceIndex::new(100);
        assert!(index.vwap(region(), item()).is_none());
    }

    #[test]
    fn window_size_accessor_returns_configured_value() {
        let index = MarketPriceIndex::new(42);
        assert_eq!(index.window_size(), 42);
        assert_eq!(MarketPriceIndex::DEFAULT_WINDOW_SIZE, 100);
    }

    #[test]
    fn trade_without_region_is_a_compile_error() {
        // O tipo `RegionId` é exigido por `record_trade` — este teste
        // documenta a restrição. Tentar passar `Option<RegionId>` ou
        // omitir o argumento não compila. Fail-closed pelo sistema de
        // tipos, não por runtime check.
        let mut index = MarketPriceIndex::new(10);
        let r = region();
        let it = item();
        index.record_trade(r, it, Money(1), 1);
    }

    #[test]
    fn many_small_trades_evict_oldest_completely() {
        let mut index = MarketPriceIndex::new(30);
        let r = region();
        let it = item();
        // 5 trades de 10 unidades = 50 unidades; janela 30 mantém os 3
        // últimos (30 unidades). VWAP = (10 + 20 + 30) / 3 = 20g.
        index.record_trade(r, it, Money(10), 10);
        index.record_trade(r, it, Money(20), 10);
        index.record_trade(r, it, Money(30), 10);
        index.record_trade(r, it, Money(40), 10);
        index.record_trade(r, it, Money(50), 10);
        let vwap = index.vwap(r, it).expect("há trades na janela");
        assert_eq!(vwap.unit_price, Money(40));
        // 3 entries × 10 unidades = 30 unidades na janela.
        assert_eq!(vwap.sample_quantity, 30);
        assert_eq!(vwap.trade_count, 3);
    }
}
