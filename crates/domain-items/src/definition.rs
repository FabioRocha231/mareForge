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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDefinition {
    pub id: ItemDefinitionId,
    pub kind: ItemKind,
    pub max_stack: u32,   // 1 para não-fungíveis
    pub base_weight: u32, // peso unitário em gramas
    pub tags: SmallVec<[Tag; 4]>,
    pub display_name: String, // nome para UI
}

impl ItemDefinition {
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
