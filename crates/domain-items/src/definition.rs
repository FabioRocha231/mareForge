use mareforge_shared::ids::ItemDefinitionId;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemKind {
    Resource,      // fungível: madeira, ferro, etc.
    Equipment,     // não-fungível: casco, vela, arma
    Consumable,    // fungível em pequena escala
    CurrencyToken, // fungível
    Quest,         // não-fungível
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tag {
    Wood,
    Metal,
    Cloth,
    Gunpowder,
    Food,
    Fuel,
    Tool,
    QuestItem,
    // Adicione outros conforme necessidade da vertical slice; mantenha esta lista pequena.
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipmentStats {
    /// Pontos aditivos de dano de arma.
    pub damage: i32,
    /// Offsets percentuais de velocidade: cada unidade equivale a 0,01 m/s.
    pub speed: i32,
    /// Pontos aditivos de capacidade de carga.
    pub cargo: i32,
    /// Pontos aditivos de HP máximo.
    pub hp: i32,
    /// Offsets percentuais de alcance: cada unidade equivale a 0,01 m.
    pub range: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDefinition {
    pub id: ItemDefinitionId,
    pub kind: ItemKind,
    /// Modificadores de stats — presente somente quando `kind` é `Equipment`.
    /// Fonte de verdade dos modificadores; não se duplica em instâncias.
    pub equipment: Option<EquipmentStats>,
    pub max_stack: u32,
    pub base_weight: u32,
    pub tags: SmallVec<[Tag; 4]>,
    pub display_name: String,
}

impl ItemDefinition {
    pub fn equipment(
        id: ItemDefinitionId,
        display_name: String,
        base_weight: u32,
        stats: EquipmentStats,
    ) -> Self {
        Self {
            id,
            kind: ItemKind::Equipment,
            equipment: Some(stats),
            max_stack: 1,
            base_weight,
            tags: SmallVec::new(),
            display_name,
        }
    }

    pub fn is_fungible(&self) -> bool {
        matches!(
            self.kind,
            ItemKind::Resource | ItemKind::CurrencyToken | ItemKind::Consumable
        )
    }

    pub fn is_equipment(&self) -> bool {
        matches!(self.kind, ItemKind::Equipment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(kind: ItemKind) -> ItemDefinition {
        ItemDefinition {
            id: ItemDefinitionId::new(),
            kind,
            equipment: None,
            max_stack: 1,
            base_weight: 1,
            tags: SmallVec::new(),
            display_name: String::new(),
        }
    }

    #[test]
    fn is_fungible_classifies_kinds() {
        for kind in [
            ItemKind::Resource,
            ItemKind::CurrencyToken,
            ItemKind::Consumable,
        ] {
            assert!(def(kind).is_fungible());
        }

        for kind in [ItemKind::Equipment, ItemKind::Quest] {
            assert!(!def(kind).is_fungible());
        }
    }
}
