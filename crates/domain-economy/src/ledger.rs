//! Ledger de moeda (PRD MF-026, ADR de ledger imutável). Toda entrada de
//! ouro no mundo é registrada: mint (bootstrap/faucet), burn (fees e taxas)
//! e trade (pago pelo comprador ao vendedor). Append-only: entra, nunca
//! sai — observabilidade econômica começa aqui (§71).

use serde::{Deserialize, Serialize};

use crate::currency::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LedgerKind {
    /// Ouro criado (bootstrap dev, §48; faucet de bounty no futuro).
    Mint,
    /// Ouro destruído (listing fee, transaction tax — §46/§47).
    Burn,
    /// Ouro que trocou de dono numa execução de order.
    Trade,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub seq: u64,
    pub kind: LedgerKind,
    pub amount: Money,
    /// Nota de auditoria curta (ex.: "listing fee order 7").
    pub memo: String,
}

/// Append-only: `record` é a única mutação; nada remove nem edita.
#[derive(Debug, Clone, Default)]
pub struct Ledger {
    entries: Vec<LedgerEntry>,
}

impl Ledger {
    pub fn record(
        &mut self,
        kind: LedgerKind,
        amount: Money,
        memo: impl Into<String>,
    ) -> LedgerEntry {
        let entry = LedgerEntry {
            seq: self.entries.len() as u64,
            kind,
            amount,
            memo: memo.into(),
        };
        self.entries.push(entry.clone());
        entry
    }

    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    /// Total queimado desde o gênese (§71: gold_burned).
    pub fn burned(&self) -> Money {
        Money(
            self.entries
                .iter()
                .filter(|entry| entry.kind == LedgerKind::Burn)
                .map(|entry| entry.amount.0)
                .sum(),
        )
    }

    /// Total cunhado desde o gênese (§71: gold_minted).
    pub fn minted(&self) -> Money {
        Money(
            self.entries
                .iter()
                .filter(|entry| entry.kind == LedgerKind::Mint)
                .map(|entry| entry.amount.0)
                .sum(),
        )
    }

    /// Volume executado no mercado (§71: market_volume).
    pub fn market_volume(&self) -> Money {
        Money(
            self.entries
                .iter()
                .filter(|entry| entry.kind == LedgerKind::Trade)
                .map(|entry| entry.amount.0)
                .sum(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_appends_with_monotonic_seq() {
        let mut ledger = Ledger::default();
        let first = ledger.record(LedgerKind::Mint, Money(1_000), "bootstrap");
        let second = ledger.record(LedgerKind::Burn, Money(5), "listing fee");

        assert_eq!(first.seq, 0);
        assert_eq!(second.seq, 1);
        assert_eq!(ledger.entries().len(), 2);
    }

    #[test]
    fn sums_track_kinds_separately() {
        let mut ledger = Ledger::default();
        ledger.record(LedgerKind::Mint, Money(1_000), "bootstrap a");
        ledger.record(LedgerKind::Mint, Money(1_000), "bootstrap b");
        ledger.record(LedgerKind::Burn, Money(5), "listing fee");
        ledger.record(LedgerKind::Trade, Money(485), "order 0");
        ledger.record(LedgerKind::Burn, Money(15), "transaction tax");

        assert_eq!(ledger.minted(), Money(2_000));
        assert_eq!(ledger.burned(), Money(20));
        assert_eq!(ledger.market_volume(), Money(485));
    }

    #[test]
    fn fresh_ledger_is_all_zero() {
        let ledger = Ledger::default();
        assert_eq!(ledger.minted(), Money(0));
        assert_eq!(ledger.burned(), Money(0));
        assert_eq!(ledger.market_volume(), Money(0));
    }
}
