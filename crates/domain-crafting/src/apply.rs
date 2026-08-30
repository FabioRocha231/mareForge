//! Aplicação de receitas (PRD §37, MF-021). `can_craft` julga; `craft`
//! executa: consome ingredientes do porão e produz o output — tudo-ou-nada.
//! A checagem de peso do output acontece ANTES de consumir nada: porão que
//! não comporta o resultado não perde os ingredientes.

use std::collections::HashMap;

use mareforge_domain_items::{CargoHold, ItemCatalog, ItemInstance};
use mareforge_shared::ids::ItemInstanceId;

use crate::recipe::{Recipe, StationKind};
use crate::validate::{can_craft, CraftError, InventoryView};

/// Durabilidade de saída de equipamento dev (§39: tuning é conteúdo).
const DEV_EQUIPMENT_DURABILITY: u16 = 100;

/// Executa a receita sobre o porão. Puro no sentido de regra: toda
/// invalidade é erro ANTES de qualquer mutação (fail-closed, §37).
pub fn craft(
    recipe: &Recipe,
    hold: &mut CargoHold,
    catalog: &ItemCatalog,
    station: StationKind,
) -> Result<ItemInstance, CraftError> {
    let mut quantities: HashMap<mareforge_shared::ids::ItemDefinitionId, u32> = HashMap::new();
    for custody in hold.items() {
        *quantities.entry(custody.instance.definition).or_insert(0) += custody.instance.quantity;
    }
    // Definições relevantes para a validação: output e ingredientes. Output
    // ausente do catálogo cai no UnknownOutputItem de `can_craft`.
    let mut definitions = HashMap::new();
    let mut reference = |id: mareforge_shared::ids::ItemDefinitionId| {
        if let Some(definition) = catalog.get(id) {
            definitions.insert(id, definition);
        }
    };
    reference(recipe.output_item);
    for ingredient in &recipe.ingredients {
        reference(ingredient.item);
    }

    let inventory = InventoryView {
        quantities,
        free_slots: u32::MAX, // porão não tem limite de slots, só de peso
        definitions,
        station,
    };
    can_craft(recipe, &inventory)?;

    // A saída existe no catálogo (can_craft garante): monta a instância.
    let output_definition = catalog
        .get(recipe.output_item)
        .expect("can_craft garante output conhecido");
    let output = if output_definition.is_equipment() {
        ItemInstance::new_equipment(
            ItemInstanceId::new(),
            recipe.output_item,
            DEV_EQUIPMENT_DURABILITY,
        )
    } else {
        ItemInstance::new_resource(
            ItemInstanceId::new(),
            recipe.output_item,
            recipe.output_quantity,
        )
    };

    // Peso da saída cabe? Confere ANTES de consumir: nada se perde no meio.
    hold.can_accept(catalog, output.definition, output.quantity)?;

    // Consome e produz. As remoções não podem falhar: `can_craft` validou
    // as quantidades totais há duas linhas.
    for ingredient in &recipe.ingredients {
        hold.remove(ingredient.item, ingredient.quantity)
            .expect("can_craft validou os ingredientes");
    }
    hold.insert(catalog, output.clone())
        .expect("can_accept conferiu o peso acima");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use mareforge_domain_items::{CargoHold, ItemCatalog, ItemDefinition, ItemInstance, ItemKind};
    use mareforge_shared::ids::{ItemDefinitionId, ItemInstanceId, RecipeId, ShipInstanceId};

    use super::craft;
    use crate::recipe::{Ingredient, Recipe, StationKind};

    fn resource(id: ItemDefinitionId, name: &str, weight: u32) -> ItemDefinition {
        ItemDefinition {
            id,
            kind: ItemKind::Resource,
            equipment: None,
            max_stack: 100,
            base_weight: weight,
            tags: SmallVec::new(),
            display_name: String::from(name),
        }
    }

    fn equipment(id: ItemDefinitionId, name: &str, weight: u32) -> ItemDefinition {
        ItemDefinition {
            id,
            kind: ItemKind::Equipment,
            equipment: Some(mareforge_domain_items::EquipmentStats::default()),
            max_stack: 1,
            base_weight: weight,
            tags: SmallVec::new(),
            display_name: String::from(name),
        }
    }

    fn catalog_with(definitions: &[ItemDefinition]) -> ItemCatalog {
        let mut catalog = ItemCatalog::default();
        for definition in definitions {
            catalog.register(definition.clone()).unwrap();
        }
        catalog
    }

    fn recipe(output: ItemDefinitionId, ingredients: Vec<Ingredient>) -> Recipe {
        Recipe {
            id: RecipeId::new(),
            display_name: String::from("receita de teste"),
            output_item: output,
            output_quantity: 1,
            ingredients,
            required_station: StationKind::Workbench,
            craft_time_secs: 0,
        }
    }

    #[test]
    fn craft_consumes_ingredients_and_produces_output() {
        let wood = ItemDefinitionId::new();
        let hull = ItemDefinitionId::new();
        let catalog = catalog_with(&[resource(wood, "Madeira", 2), equipment(hull, "Casco", 8)]);
        let mut hold = CargoHold::new(ShipInstanceId::new(), 100);
        hold.insert(
            &catalog,
            ItemInstance::new_resource(ItemInstanceId::new(), wood, 20),
        )
        .unwrap();

        let output = craft(
            &recipe(
                hull,
                vec![Ingredient {
                    item: wood,
                    quantity: 15,
                }],
            ),
            &mut hold,
            &catalog,
            StationKind::Workbench,
        )
        .unwrap();

        assert_eq!(output.definition, hull);
        assert!(output.durability.is_some());
        // 40 de peso de madeira - 30 consumidos + 8 do casco = 18.
        assert_eq!(hold.used_weight(&catalog).unwrap(), 18);
    }

    #[test]
    fn craft_without_station_fails_and_consumes_nothing() {
        let wood = ItemDefinitionId::new();
        let hull = ItemDefinitionId::new();
        let catalog = catalog_with(&[resource(wood, "Madeira", 2), equipment(hull, "Casco", 8)]);
        let mut hold = CargoHold::new(ShipInstanceId::new(), 100);
        hold.insert(
            &catalog,
            ItemInstance::new_resource(ItemInstanceId::new(), wood, 20),
        )
        .unwrap();

        let error = craft(
            &recipe(
                hull,
                vec![Ingredient {
                    item: wood,
                    quantity: 15,
                }],
            ),
            &mut hold,
            &catalog,
            crate::recipe::StationKind::None,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            crate::validate::CraftError::WrongStation { .. }
        ));
        assert_eq!(hold.used_weight(&catalog).unwrap(), 40); // intacto
    }

    #[test]
    fn craft_is_atomic_when_output_would_overflow_weight() {
        let wood = ItemDefinitionId::new();
        let hull = ItemDefinitionId::new();
        // Casco pesa 60: porão de 90 com 40 de madeira deixa 50 livres —
        // consumir 15 (30 de peso) caberia, mas o casco de 60 não.
        let catalog = catalog_with(&[resource(wood, "Madeira", 2), equipment(hull, "Casco", 60)]);
        let mut hold = CargoHold::new(ShipInstanceId::new(), 90);
        hold.insert(
            &catalog,
            ItemInstance::new_resource(ItemInstanceId::new(), wood, 20),
        )
        .unwrap();

        let error = craft(
            &recipe(
                hull,
                vec![Ingredient {
                    item: wood,
                    quantity: 5,
                }],
            ),
            &mut hold,
            &catalog,
            StationKind::Workbench,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            crate::validate::CraftError::Cargo(
                mareforge_domain_items::CargoError::CargoCapacityExceeded { .. }
            )
        ));
        // Nada foi consumido: o porão segue com as 20 madeiras inteiras.
        assert_eq!(hold.used_weight(&catalog).unwrap(), 40);
        assert_eq!(hold.items().len(), 1);
    }
}
