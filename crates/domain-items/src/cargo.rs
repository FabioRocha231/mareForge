//! Porão de navio (PRD §28/§59): itens em `ItemLocation::ShipCargo`, com
//! limite de peso derivado de `ShipStats.cargo_capacity` e peso unitário das
//! definições. Toda operação que estoura capacidade **falha** — nunca clamp
//! silencioso.

use crate::catalog::ItemCatalog;
use crate::instance::ItemInstance;
use crate::location::{Custody, ItemLocation};
use mareforge_shared::ids::ShipInstanceId;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CargoError {
    #[error("cargo capacity exceeded: needs {needed} but hold allows {available}")]
    CargoCapacityExceeded { needed: u32, available: u32 },
    #[error("item {0:?} is not in the catalog")]
    UnknownItem(mareforge_shared::ids::ItemDefinitionId),
    #[error("quantity {requested} not available in cargo")]
    NotEnoughItems { requested: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CargoHold {
    ship: ShipInstanceId,
    capacity: u32,
    slots: Vec<Custody>,
}

impl CargoHold {
    /// Porão novo e vazio amarrado ao seu navio.
    pub fn new(ship: ShipInstanceId, capacity: u32) -> Self {
        Self {
            ship,
            capacity,
            slots: Vec::new(),
        }
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn items(&self) -> &[Custody] {
        &self.slots
    }

    /// Peso total carregado. Fail-closed: definição ausente no catálogo é erro.
    pub fn used_weight(&self, catalog: &ItemCatalog) -> Result<u32, CargoError> {
        let mut total = 0u32;
        for custody in &self.slots {
            let def = catalog
                .get(custody.instance.definition)
                .ok_or(CargoError::UnknownItem(custody.instance.definition))?;
            total = total.saturating_add(def.base_weight.saturating_mul(custody.instance.quantity));
        }
        Ok(total)
    }

    pub fn free_weight(&self, catalog: &ItemCatalog) -> Result<u32, CargoError> {
        Ok(self.capacity.saturating_sub(self.used_weight(catalog)?))
    }

    /// Verifica se uma quantidade de uma definição cabe no porão.
    pub fn can_accept(
        &self,
        catalog: &ItemCatalog,
        definition: mareforge_shared::ids::ItemDefinitionId,
        quantity: u32,
    ) -> Result<(), CargoError> {
        let def = catalog
            .get(definition)
            .ok_or(CargoError::UnknownItem(definition))?;
        let needed = def.base_weight.saturating_mul(quantity);
        let free = self.free_weight(catalog)?;
        if needed > free {
            return Err(CargoError::CargoCapacityExceeded {
                needed,
                available: free,
            });
        }
        Ok(())
    }

    /// Insere uma instância no porão. Se couber no peso, a custódia é
    /// regravada como `ShipCargo` deste navio — a localização acompanha o
    /// contêiner, nunca o contrário.
    pub fn insert(
        &mut self,
        catalog: &ItemCatalog,
        instance: ItemInstance,
    ) -> Result<(), CargoError> {
        self.can_accept(catalog, instance.definition, instance.quantity)?;
        self.slots
            .push(Custody::new(instance, ItemLocation::ShipCargo(self.ship)));
        Ok(())
    }

    /// Transfere atômicamente tudo de uma custódia externa (ex.: baú de
    /// wreck) para o porão. Nada é transferido se qualquer item não couber
    /// (PRD MF-015: atômico e capacity-aware).
    pub fn take_all(
        &mut self,
        catalog: &ItemCatalog,
        incoming: Vec<Custody>,
    ) -> Result<Vec<Custody>, CargoError> {
        let mut needed = 0u32;
        for custody in &incoming {
            let def = catalog
                .get(custody.instance.definition)
                .ok_or(CargoError::UnknownItem(custody.instance.definition))?;
            needed =
                needed.saturating_add(def.base_weight.saturating_mul(custody.instance.quantity));
        }
        let free = self.free_weight(catalog)?;
        if needed > free {
            return Err(CargoError::CargoCapacityExceeded {
                needed,
                available: free,
            });
        }

        let mut moved = Vec::with_capacity(incoming.len());
        for custody in incoming {
            moved.push(Custody::new(
                custody.instance,
                ItemLocation::ShipCargo(self.ship),
            ));
        }
        self.slots.extend(moved.iter().cloned());
        Ok(moved)
    }

    /// Esvazia o porão inteiro, devolvendo as custódias (depositar tudo no
    /// storage regional). As localizações seguem `ShipCargo` do navio — o
    /// contêiner de destino regrava ao receber.
    pub fn drain(&mut self) -> Vec<Custody> {
        std::mem::take(&mut self.slots)
    }

    /// Retira `quantity` unidades de uma definição (primeiro slot que atenda).
    pub fn remove(
        &mut self,
        definition: mareforge_shared::ids::ItemDefinitionId,
        quantity: u32,
    ) -> Result<ItemInstance, CargoError> {
        let Some(position) = self
            .slots
            .iter()
            .position(|custody| custody.instance.definition == definition)
        else {
            return Err(CargoError::NotEnoughItems {
                requested: quantity,
            });
        };
        let slot = &mut self.slots[position];
        if slot.instance.quantity < quantity {
            return Err(CargoError::NotEnoughItems {
                requested: quantity,
            });
        }
        slot.instance.quantity -= quantity;
        let taken = ItemInstance {
            id: slot.instance.id,
            definition,
            quantity,
            durability: slot.instance.durability,
        };
        if slot.instance.quantity == 0 {
            self.slots.remove(position);
        }
        Ok(taken)
    }
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use mareforge_shared::ids::{ItemDefinitionId, ItemInstanceId, ShipInstanceId, WreckId};

    use super::*;
    use crate::catalog::ItemCatalog;
    use crate::definition::{ItemDefinition, ItemKind};
    use crate::instance::ItemInstance;
    use crate::location::ItemLocation;

    fn catalog_with_timber(weight: u32) -> (ItemCatalog, ItemDefinitionId) {
        let id = ItemDefinitionId::new();
        let mut catalog = ItemCatalog::default();
        catalog
            .register(ItemDefinition {
                id,
                kind: ItemKind::Resource,
                equipment: None,
                max_stack: 100,
                base_weight: weight,
                tags: SmallVec::new(),
                display_name: String::from("Timber"),
            })
            .unwrap();
        (catalog, id)
    }

    fn instance(def: ItemDefinitionId, quantity: u32) -> ItemInstance {
        ItemInstance::new_resource(ItemInstanceId::new(), def, quantity)
    }

    #[test]
    fn empty_hold_has_full_free_weight() {
        let (catalog, _) = catalog_with_timber(2);
        let hold = CargoHold::new(ShipInstanceId::new(), 100);

        assert_eq!(hold.used_weight(&catalog).unwrap(), 0);
        assert_eq!(hold.free_weight(&catalog).unwrap(), 100);
    }

    #[test]
    fn insert_records_ship_cargo_location_and_weight() {
        let (catalog, timber) = catalog_with_timber(2);
        let ship = ShipInstanceId::new();
        let mut hold = CargoHold::new(ship, 100);

        hold.insert(&catalog, instance(timber, 30)).unwrap();

        assert_eq!(hold.used_weight(&catalog).unwrap(), 60);
        assert_eq!(hold.items()[0].location, ItemLocation::ShipCargo(ship));
    }

    #[test]
    fn insert_over_capacity_fails_with_capacity_exceeded() {
        let (catalog, timber) = catalog_with_timber(2);
        let mut hold = CargoHold::new(ShipInstanceId::new(), 100);

        let error = hold.insert(&catalog, instance(timber, 51)).unwrap_err();

        assert_eq!(
            error,
            CargoError::CargoCapacityExceeded {
                needed: 102,
                available: 100
            }
        );
    }

    #[test]
    fn weight_of_unknown_definition_fails_closed() {
        let catalog = ItemCatalog::default();
        // Inserido por outra via (sem catálogo): peso de item desconhecido não
        // pode ser assumido como zero.
        let mut hold = CargoHold::new(ShipInstanceId::new(), 100);
        hold.slots.push(Custody::new(
            instance(ItemDefinitionId::new(), 10),
            ItemLocation::ShipCargo(ShipInstanceId::new()),
        ));

        assert!(matches!(
            hold.used_weight(&catalog),
            Err(CargoError::UnknownItem(_))
        ));
    }

    #[test]
    fn take_all_is_atomic_when_anything_does_not_fit() {
        let (catalog, timber) = catalog_with_timber(2);
        let ship = ShipInstanceId::new();
        let mut hold = CargoHold::new(ship, 100);
        hold.insert(&catalog, instance(timber, 40)).unwrap(); // 80 usados, 20 livres

        let wreck = WreckId::new();
        let incoming = vec![
            Custody::new(instance(timber, 5), ItemLocation::Wreck(wreck)),
            Custody::new(instance(timber, 6), ItemLocation::Wreck(wreck)), // 22 no total
        ];

        let error = hold.take_all(&catalog, incoming).unwrap_err();
        assert_eq!(
            error,
            CargoError::CargoCapacityExceeded {
                needed: 22,
                available: 20
            }
        );
        // Nada entrou: operação é tudo-ou-nada.
        assert_eq!(hold.items().len(), 1);
        assert_eq!(hold.used_weight(&catalog).unwrap(), 80);
    }

    #[test]
    fn take_all_moves_wreck_custody_into_ship_cargo() {
        let (catalog, timber) = catalog_with_timber(2);
        let ship = ShipInstanceId::new();
        let mut hold = CargoHold::new(ship, 100);
        let wreck = WreckId::new();
        let incoming = vec![Custody::new(
            instance(timber, 8),
            ItemLocation::Wreck(wreck),
        )];

        let moved = hold.take_all(&catalog, incoming).unwrap();

        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0].location, ItemLocation::ShipCargo(ship));
        assert_eq!(hold.items().len(), 1);
        assert_eq!(hold.items()[0].location, ItemLocation::ShipCargo(ship));
        assert_eq!(hold.used_weight(&catalog).unwrap(), 16);
    }

    #[test]
    fn drain_empties_hold_and_returns_everything() {
        let (catalog, timber) = catalog_with_timber(2);
        let mut hold = CargoHold::new(ShipInstanceId::new(), 100);
        hold.insert(&catalog, instance(timber, 10)).unwrap();
        hold.insert(&catalog, instance(timber, 5)).unwrap();

        let drained = hold.drain();

        assert_eq!(drained.len(), 2);
        assert!(hold.items().is_empty());
        assert_eq!(hold.used_weight(&catalog).unwrap(), 0);
        // Custódias drenadas ainda sabem de onde vieram (auditoria).
        assert!(drained
            .iter()
            .all(|custody| matches!(custody.location, ItemLocation::ShipCargo(_))));
    }

    #[test]
    fn remove_takes_from_stack_and_drops_empty_slot() {
        let (catalog, timber) = catalog_with_timber(1);
        let mut hold = CargoHold::new(ShipInstanceId::new(), 100);
        hold.insert(&catalog, instance(timber, 5)).unwrap();

        let taken = hold.remove(timber, 5).unwrap();

        assert_eq!(taken.quantity, 5);
        assert!(hold.items().is_empty());
        assert!(hold.remove(timber, 1).is_err());
    }
}
