//! Guilda Mercante (NPC compradora) por porto. Pilar 1 intacto: a guilda só
//! COMPRA — o item vendido é destruído (sink) e o ouro pago é faucet
//! auditado no ledger (`LedgerKind::GuildPurchase`). Regras puras: tabela de
//! valor base, multiplicador regional e saturação com decaimento exponencial.

use std::collections::HashMap;

use crate::currency::Money;

/// Valor base por nome de exibição do item. Item fora da tabela NÃO é
/// comprável (fail-closed). Novo item = uma linha aqui.
pub const GUILD_BASE_VALUES: &[(&str, u64)] = &[
    ("Madeira", 10),
    ("Minério", 14),
    ("Coral Negro", 60),
    ("Casco Reforçado", 180),
    ("Velas de Corrida", 160),
    ("Canhão de Bronze", 220),
];

/// Multiplicadores de portos sem tabela própria (ex.: um porto pirata novo).
/// Item ausente = 1.0x.
pub const DEFAULT_REGIONAL_MULTIPLIERS: &[(&str, f64)] = &[("Coral Negro", 1.3)];

/// Multiplicadores por porto: especialidade local paga pouco, a do outro
/// porto paga muito — é daqui que nasce a rota. Item ausente = 1.0x.
pub const PORT_REGIONAL_MULTIPLIERS: &[(&str, &[(&str, f64)])] = &[
    (
        "Porto da Serra",
        &[("Madeira", 0.6), ("Minério", 1.6), ("Coral Negro", 2.0)],
    ),
    (
        "Porto da Mina",
        &[("Madeira", 1.6), ("Minério", 0.6), ("Coral Negro", 2.0)],
    ),
];

/// Meia-vida da saturação: vender muito derruba o preço, que se recupera.
pub const SATURATION_HALF_LIFE_SECS: f64 = 600.0;
/// Unidades recentes que derrubam o preço à metade.
pub const SATURATION_CAPACITY: f64 = 50.0;

pub fn base_value(item: &str) -> Option<u64> {
    GUILD_BASE_VALUES
        .iter()
        .find(|(name, _)| *name == item)
        .map(|(_, value)| *value)
}

pub fn regional_multiplier(port: &str, item: &str) -> f64 {
    let table = PORT_REGIONAL_MULTIPLIERS
        .iter()
        .find(|(name, _)| *name == port)
        .map(|(_, table)| *table)
        .unwrap_or(DEFAULT_REGIONAL_MULTIPLIERS);
    table
        .iter()
        .find(|(name, _)| *name == item)
        .map(|(_, multiplier)| *multiplier)
        .unwrap_or(1.0)
}

/// Valor sem saturação (base × regional) — referência para recompensas.
pub fn guild_value(port: &str, item: &str) -> Option<f64> {
    base_value(item).map(|base| base as f64 * regional_multiplier(port, item))
}

/// Preço de UMA unidade com `recent_units` já vendidas recentemente.
fn unit_price_at(value: f64, recent_units: f64) -> Money {
    Money(((value / (1.0 + recent_units / SATURATION_CAPACITY)).floor() as u64).max(1))
}

/// Unidades vendidas recentemente numa (porto, item), com instante da
/// última atualização para o decaimento.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Demand {
    units: f64,
    at_secs: f64,
}

impl Demand {
    fn units_at(&self, now_secs: f64) -> f64 {
        let elapsed = (now_secs - self.at_secs).max(0.0);
        self.units * 0.5_f64.powf(elapsed / SATURATION_HALF_LIFE_SECS)
    }
}

/// Livro da guilda: saturação por (porto, item). Tempo em segundos
/// monotônicos do servidor.
#[derive(Debug, Clone, Default)]
pub struct GuildBook {
    demand: HashMap<(String, String), Demand>,
}

impl GuildBook {
    fn recent(&self, port: &str, item: &str, now_secs: f64) -> f64 {
        self.demand
            .get(&(port.to_owned(), item.to_owned()))
            .map(|demand| demand.units_at(now_secs))
            .unwrap_or(0.0)
    }

    /// Preço da próxima unidade. `None` = a guilda não compra (fail-closed).
    pub fn unit_price(&self, port: &str, item: &str, now_secs: f64) -> Option<Money> {
        let value = guild_value(port, item)?;
        Some(unit_price_at(value, self.recent(port, item, now_secs)))
    }

    /// Total por `quantity` unidades: cada unidade satura a seguinte.
    pub fn quote(&self, port: &str, item: &str, quantity: u32, now_secs: f64) -> Option<Money> {
        let value = guild_value(port, item)?;
        let recent = self.recent(port, item, now_secs);
        Some(Money(
            (0..quantity)
                .map(|sold| unit_price_at(value, recent + f64::from(sold)).0)
                .sum(),
        ))
    }

    pub fn record_sale(&mut self, port: &str, item: &str, quantity: u32, now_secs: f64) {
        let units = self.recent(port, item, now_secs) + f64::from(quantity);
        self.demand.insert(
            (port.to_owned(), item.to_owned()),
            Demand {
                units,
                at_secs: now_secs,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_item_is_not_buyable() {
        let book = GuildBook::default();
        assert_eq!(book.unit_price("Porto da Serra", "Pedra Magica", 0.0), None);
        assert_eq!(book.quote("Porto da Serra", "Pedra Magica", 3, 0.0), None);
    }

    #[test]
    fn regional_multipliers_create_arbitrage() {
        let book = GuildBook::default();
        assert_eq!(
            book.unit_price("Porto da Serra", "Madeira", 0.0),
            Some(Money(6))
        );
        assert_eq!(
            book.unit_price("Porto da Mina", "Madeira", 0.0),
            Some(Money(16))
        );
        assert_eq!(
            book.unit_price("Porto da Serra", "Minério", 0.0),
            Some(Money(22))
        );
        assert_eq!(
            book.unit_price("Porto da Mina", "Coral Negro", 0.0),
            Some(Money(120))
        );
        assert_eq!(
            book.unit_price("Porto da Mina", "Canhão de Bronze", 0.0),
            Some(Money(220))
        );
    }

    #[test]
    fn unknown_port_falls_back_to_default_table() {
        assert_eq!(
            regional_multiplier("Porto do Coral Negro", "Coral Negro"),
            1.3
        );
        assert_eq!(regional_multiplier("Porto do Coral Negro", "Madeira"), 1.0);
    }

    #[test]
    fn selling_saturates_and_price_recovers_over_time() {
        let mut book = GuildBook::default();
        let fresh = book
            .unit_price("Porto da Mina", "Coral Negro", 0.0)
            .unwrap();
        book.record_sale("Porto da Mina", "Coral Negro", 50, 0.0);
        // 50 unidades recentes = capacidade: preço cai à metade.
        assert_eq!(
            book.unit_price("Porto da Mina", "Coral Negro", 0.0),
            Some(Money(fresh.0 / 2))
        );
        // Uma meia-vida depois: 25 unidades → 1/(1+0.5).
        assert_eq!(
            book.unit_price("Porto da Mina", "Coral Negro", SATURATION_HALF_LIFE_SECS),
            Some(Money(80))
        );
        // Outro porto não é afetado.
        assert_eq!(
            book.unit_price("Porto da Serra", "Coral Negro", 0.0),
            Some(fresh)
        );
    }

    #[test]
    fn quote_saturates_within_the_same_sale() {
        let book = GuildBook::default();
        let one = book.quote("Porto da Serra", "Minério", 1, 0.0).unwrap();
        let many = book.quote("Porto da Serra", "Minério", 100, 0.0).unwrap();
        assert!(many.0 < one.0 * 100);
        assert!(many.0 > one.0 * 50);
    }
}
