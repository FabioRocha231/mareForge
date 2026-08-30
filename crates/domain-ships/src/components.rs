use mareforge_shared::ids::ItemDefinitionId;
use serde::{Deserialize, Serialize};

use crate::definition::SlotKind;

/// Componente equipado em um slot. Apenas o ID da definição por enquanto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquippedComponent {
    pub slot_kind: SlotKind,
    pub item_definition: ItemDefinitionId,
    /// Modificadores derivados do item. Como `domain-items` não carrega stats de equipamento
    /// ainda, usamos um struct plano aqui — futuro refator vai puxar de `domain-items`.
    ///
    /// Valores negativos pioram o stat. Estes campos não são persistidos e serão lidos de
    /// `ItemDefinition` quando o catálogo tiver stats ricos.
    pub damage_modifier: i32,
    pub speed_modifier: i32,
    pub cargo_modifier: i32,
    pub hp_modifier: i32,
    pub range_modifier: i32,
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
