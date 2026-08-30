//! Risco (PRD §8-§9): três tiers, uma única versão de full loot. A diferença
//! entre Frontier e Lawless é geografia e oportunidade — nunca regras secretas
//! de proteção.

use serde::{Deserialize, Serialize};

/// Tier de risco de uma zona (PRD §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RiskTier {
    /// PvP desativado; economia de sobrevivência (recompensa baixa).
    Protected,
    /// PvP com full loot; rotas comerciais e recursos intermediários.
    Frontier,
    /// PvP com full loot; recursos raros e melhores oportunidades.
    Lawless,
}

impl RiskTier {
    /// Este tier carrega PvP em si (independe da política)? Frontier e
    /// Lawless são zonas de PvP por natureza (§9: full loot é um só).
    pub fn is_pvp(self) -> bool {
        !matches!(self, RiskTier::Protected)
    }
}

/// A política que traduz tier em regra de combate (Pilar 4: o servidor é a
/// lei, e a lei é escrita aqui — não espalhada por sistemas).
///
/// `protected_pvp` existe como switch explícito de configuração; o Vertical
/// Slice nunca o liga (PRD §8). PvP em Frontier/Lawless não é negociável.
/// Default = `protected_pvp: false` (fail-closed: o seguro nasce desligado).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RiskPolicy {
    pub protected_pvp: bool,
}

impl RiskPolicy {
    /// Combate PvP é permitido neste tier?
    pub fn pvp_allowed(&self, tier: RiskTier) -> bool {
        match tier {
            RiskTier::Protected => self.protected_pvp,
            // §9: entrou em região PvP, é full loot — sem meias-medidas.
            RiskTier::Frontier | RiskTier::Lawless => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_denies_pvp_by_default() {
        let policy = RiskPolicy::default();
        assert!(!policy.pvp_allowed(RiskTier::Protected));
    }

    #[test]
    fn protected_switch_is_explicit_configuration() {
        let policy = RiskPolicy {
            protected_pvp: true,
        };
        assert!(policy.pvp_allowed(RiskTier::Protected));
    }

    #[test]
    fn pvp_tiers_allow_unconditionally_no_second_full_loot() {
        let policy = RiskPolicy::default();
        assert!(policy.pvp_allowed(RiskTier::Frontier));
        assert!(policy.pvp_allowed(RiskTier::Lawless));
    }

    #[test]
    fn tier_nature_matches_policy_default() {
        assert!(!RiskTier::Protected.is_pvp());
        assert!(RiskTier::Frontier.is_pvp());
        assert!(RiskTier::Lawless.is_pvp());
    }

    #[test]
    fn tier_roundtrips_serde() {
        for tier in [RiskTier::Protected, RiskTier::Frontier, RiskTier::Lawless] {
            let bytes = bincode::serialize(&tier).unwrap();
            assert_eq!(bincode::deserialize::<RiskTier>(&bytes).unwrap(), tier);
        }
    }
}
