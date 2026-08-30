use std::collections::HashMap;

use mareforge_shared::ids::ItemDefinitionId;
use thiserror::Error;

use crate::definition::{ItemDefinition, ItemKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CatalogError {
    #[error("item {0:?} is not in the catalog")]
    UnknownItem(ItemDefinitionId),
    #[error("item {0:?} is not equipment")]
    NotEquipment(ItemDefinitionId),
    #[error("duplicate item definition {0:?}")]
    DuplicateItem(ItemDefinitionId),
    #[error("equipment definition {0:?} must carry equipment stats")]
    EquipmentWithoutStats(ItemDefinitionId),
    #[error("non-equipment definition {0:?} must not carry equipment stats")]
    NonEquipmentWithStats(ItemDefinitionId),
}

/// Catálogo de definições conhecidas pelo servidor. Fail-closed: registro
/// rejeita definições que violam o invariante equipment/stats, e consultas
/// por item ausente ou não-equipamento retornam erro — nunca default.
#[derive(Debug, Clone, Default)]
pub struct ItemCatalog {
    definitions: HashMap<ItemDefinitionId, ItemDefinition>,
}

impl ItemCatalog {
    pub fn register(&mut self, definition: ItemDefinition) -> Result<(), CatalogError> {
        if definition.kind == ItemKind::Equipment && definition.equipment.is_none() {
            return Err(CatalogError::EquipmentWithoutStats(definition.id));
        }
        if definition.kind != ItemKind::Equipment && definition.equipment.is_some() {
            return Err(CatalogError::NonEquipmentWithStats(definition.id));
        }

        let id = definition.id;
        if self.definitions.contains_key(&id) {
            return Err(CatalogError::DuplicateItem(id));
        }
        self.definitions.insert(id, definition);
        Ok(())
    }

    pub fn get(&self, id: ItemDefinitionId) -> Option<&ItemDefinition> {
        self.definitions.get(&id)
    }

    pub fn equipment_stats(
        &self,
        id: ItemDefinitionId,
    ) -> Result<&crate::definition::EquipmentStats, CatalogError> {
        let definition = self
            .definitions
            .get(&id)
            .ok_or(CatalogError::UnknownItem(id))?;
        definition
            .equipment
            .as_ref()
            .ok_or(CatalogError::NotEquipment(id))
    }
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::definition::EquipmentStats;

    fn resource(id: ItemDefinitionId) -> ItemDefinition {
        ItemDefinition {
            id,
            kind: ItemKind::Resource,
            equipment: None,
            max_stack: 10,
            base_weight: 100,
            tags: SmallVec::new(),
            display_name: String::new(),
        }
    }

    fn weapon(id: ItemDefinitionId) -> ItemDefinition {
        ItemDefinition::equipment(
            id,
            String::from("iron cannon"),
            250,
            EquipmentStats {
                damage: 15,
                ..EquipmentStats::default()
            },
        )
    }

    #[test]
    fn register_then_get_returns_definition() {
        let id = ItemDefinitionId::new();
        let mut catalog = ItemCatalog::default();

        catalog.register(resource(id)).unwrap();

        assert_eq!(catalog.get(id).unwrap().id, id);
    }

    #[test]
    fn register_rejects_duplicate_id() {
        let id = ItemDefinitionId::new();
        let mut catalog = ItemCatalog::default();
        catalog.register(resource(id)).unwrap();

        assert_eq!(
            catalog.register(resource(id)),
            Err(CatalogError::DuplicateItem(id))
        );
    }

    #[test]
    fn register_rejects_equipment_without_stats() {
        let id = ItemDefinitionId::new();
        let mut catalog = ItemCatalog::default();
        let def = ItemDefinition {
            id,
            kind: ItemKind::Equipment,
            equipment: None,
            max_stack: 1,
            base_weight: 1,
            tags: SmallVec::new(),
            display_name: String::new(),
        };

        assert_eq!(
            catalog.register(def),
            Err(CatalogError::EquipmentWithoutStats(id))
        );
    }

    #[test]
    fn register_rejects_non_equipment_with_stats() {
        let id = ItemDefinitionId::new();
        let mut catalog = ItemCatalog::default();
        let mut def = resource(id);
        def.equipment = Some(EquipmentStats::default());

        assert_eq!(
            catalog.register(def),
            Err(CatalogError::NonEquipmentWithStats(id))
        );
    }

    #[test]
    fn equipment_stats_returns_stats_for_equipment() {
        let id = ItemDefinitionId::new();
        let mut catalog = ItemCatalog::default();
        catalog.register(weapon(id)).unwrap();

        assert_eq!(catalog.equipment_stats(id).unwrap().damage, 15);
    }

    #[test]
    fn equipment_stats_fails_closed_for_unknown_item() {
        let catalog = ItemCatalog::default();
        let id = ItemDefinitionId::new();

        assert_eq!(
            catalog.equipment_stats(id),
            Err(CatalogError::UnknownItem(id))
        );
    }

    #[test]
    fn equipment_stats_rejects_non_equipment() {
        let id = ItemDefinitionId::new();
        let mut catalog = ItemCatalog::default();
        catalog.register(resource(id)).unwrap();

        assert_eq!(
            catalog.equipment_stats(id),
            Err(CatalogError::NotEquipment(id))
        );
    }
}
