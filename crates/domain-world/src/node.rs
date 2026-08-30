//! ResourceNode (PRD §57, MF-018): a matéria-prima do mundo. O node é
//! server-authoritative — o client vê e clica, o servidor decide. Distribuição
//! regional (MF-020) é geografia: cada node pertence a uma região, e região
//! rara fica longe e sem lei (Pilar 3).

use mareforge_shared::ids::{ItemDefinitionId, RegionId, ResourceNodeId};

/// Um depósito de recurso no mar: posição fixa, estoque finito, resposta
/// regional. O `resource` é o `ItemDefinitionId` que o servidor entrega por
/// coleta — o catálogo mora fora daqui (ADR-0006).
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceNode {
    pub id: ResourceNodeId,
    pub name: &'static str,
    pub x: f32,
    pub y: f32,
    /// Região econômica dona deste depósito (distribuição regional, MF-020).
    pub region: RegionId,
    pub resource: ItemDefinitionId,
    /// Unidades disponíveis agora. Zerado = esgotado até respawn.
    pub stock: u32,
    pub max_stock: u32,
}

impl ResourceNode {
    /// O navio em (`x`, `y`) está perto o bastante para coletar?
    pub fn in_range(&self, x: f32, y: f32, radius: f32) -> bool {
        let dx = x - self.x;
        let dy = y - self.y;
        dx * dx + dy * dy <= radius * radius
    }

    pub fn is_depleted(&self) -> bool {
        self.stock == 0
    }

    /// Toma do depósito até `requested` unidades — o que houver. O servidor
    /// pergunta ao porão quanto cabe ANTES de tomar: nada se perde no mar.
    pub fn take(&mut self, requested: u32) -> u32 {
        let taken = requested.min(self.stock);
        self.stock -= taken;
        taken
    }

    /// Respawn (Phase 6): o depósito volta ao máximo. Recurso é finito no
    /// tempo, não no mundo — economia tem que continuar girando.
    pub fn refill(&mut self) {
        self.stock = self.max_stock;
    }
}

/// Tuning de coleta (valores de balanceamento vivem em configuração, PRD §23).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GatheringPolicy {
    /// Unidades por coleta (antes do corte por espaço de porão).
    pub amount_per_gather: u32,
    /// Distância máxima navio→node, em metros.
    pub interact_radius: f32,
    /// Segundos de respawn após esgotar.
    pub respawn_secs: f32,
}

impl Default for GatheringPolicy {
    fn default() -> Self {
        Self {
            amount_per_gather: 10,
            interact_radius: 30.0,
            respawn_secs: 45.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(stock: u32) -> ResourceNode {
        ResourceNode {
            id: ResourceNodeId::new(),
            name: "Bosque",
            x: 100.0,
            y: 50.0,
            region: RegionId::new(),
            resource: ItemDefinitionId::new(),
            stock,
            max_stock: stock.max(60),
        }
    }

    #[test]
    fn take_deducts_and_caps_at_stock() {
        let mut node = node(25);
        assert_eq!(node.take(10), 10);
        assert_eq!(node.stock, 15);
        // pedir mais do que tem entrega o que tem — sem pânico.
        assert_eq!(node.take(99), 15);
        assert_eq!(node.stock, 0);
    }

    #[test]
    fn depletion_and_refill_round_trip() {
        let mut node = node(5);
        node.take(5);
        assert!(node.is_depleted());
        node.refill();
        assert_eq!(node.stock, 60);
        assert!(!node.is_depleted());
    }

    #[test]
    fn range_check_matches_circle_geometry() {
        let node = node(10);
        assert!(node.in_range(100.0, 50.0, 30.0)); // centro
        assert!(node.in_range(130.0, 50.0, 30.0)); // borda exata
        assert!(!node.in_range(131.0, 50.0, 30.0));
    }

    #[test]
    fn default_policy_is_coherent() {
        let policy = GatheringPolicy::default();
        assert!(policy.amount_per_gather > 0);
        assert!(policy.interact_radius > 0.0);
        assert!(policy.respawn_secs > 0.0);
    }
}
