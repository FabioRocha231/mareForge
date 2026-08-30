use mareforge_shared::ids::ItemDefinitionId;
use serde::{Deserialize, Serialize};

use crate::definition::SlotKind;

/// Componente equipado em um slot. Aponta para a definição rica no catálogo
/// de itens (`domain-items`), fonte única dos modificadores de stats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquippedComponent {
    pub slot_kind: SlotKind,
    pub item_definition: ItemDefinitionId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquippedComponents {
    pub hull: Vec<EquippedComponent>,
    pub sail: Vec<EquippedComponent>,
    pub weapon: Vec<EquippedComponent>,
    pub aux: Vec<EquippedComponent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        let equipped = EquippedComponents::default();
        assert!(equipped.hull.is_empty());
        assert!(equipped.sail.is_empty());
        assert!(equipped.weapon.is_empty());
        assert!(equipped.aux.is_empty());
    }
}
