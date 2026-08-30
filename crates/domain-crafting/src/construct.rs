//! Construção de navios (PRD §38, MF-022). O loop econômico não fecha se
//! navios destruídos não podem ser reconstruídos: Dock + recursos →
//! `ShipInstance`. O navio NÃO é `ItemDefinition` — é entidade própria —,
//! então a construção tem seu próprio tipo de job e sua regra própria. A
//! criação da instância em si é do runtime (servidor); aqui mora a regra
//! pura de *permissão*: estação e ingredientes.

use std::collections::HashMap;

use mareforge_domain_ships::ShipKind;
use mareforge_shared::ids::{ItemDefinitionId, RecipeId};

use crate::recipe::{Ingredient, StationKind};
use crate::validate::CraftError;

/// Ordem de construção: o que o Dock fabrica. Espelho de `Recipe` para
/// output que não é item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShipConstructionJob {
    pub id: RecipeId,
    pub display_name: String,
    pub kind: ShipKind,
    pub ingredients: Vec<Ingredient>,
    pub required_station: StationKind,
}

/// O inventário satisfaz a ordem de construção? Puro: julga e não muta.
pub fn can_construct(
    job: &ShipConstructionJob,
    quantities: &HashMap<ItemDefinitionId, u32>,
    station: StationKind,
) -> Result<(), CraftError> {
    if job.required_station != StationKind::None && job.required_station != station {
        return Err(CraftError::WrongStation {
            required: job.required_station,
            current: station,
        });
    }
    for ingredient in &job.ingredients {
        let have = quantities.get(&ingredient.item).copied().unwrap_or(0);
        if have < ingredient.quantity {
            return Err(CraftError::MissingIngredient {
                item: ingredient.item,
                needed: ingredient.quantity,
                available: have,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use mareforge_shared::ids::{ItemDefinitionId, RecipeId};

    use super::{can_construct, ShipConstructionJob};
    use crate::recipe::{Ingredient, StationKind};
    use crate::validate::CraftError;

    fn job() -> ShipConstructionJob {
        ShipConstructionJob {
            id: RecipeId::new(),
            display_name: String::from("Patrol"),
            kind: mareforge_domain_ships::ShipKind::Patrol,
            ingredients: vec![Ingredient {
                item: ItemDefinitionId::new(),
                quantity: 30,
            }],
            required_station: StationKind::Dock,
        }
    }

    #[test]
    fn construction_requires_dock_and_ingredients() {
        let job = job();
        let mut quantities = HashMap::new();
        quantities.insert(job.ingredients[0].item, 30);

        assert_eq!(can_construct(&job, &quantities, StationKind::Dock), Ok(()));
        assert!(matches!(
            can_construct(&job, &quantities, StationKind::Workbench),
            Err(CraftError::WrongStation { .. })
        ));

        quantities.insert(job.ingredients[0].item, 29);
        assert!(matches!(
            can_construct(&job, &quantities, StationKind::Dock),
            Err(CraftError::MissingIngredient { .. })
        ));
    }
}
