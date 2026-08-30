use mareforge_shared::ids::{ItemDefinitionId, RecipeId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StationKind {
    None,
    Workbench,
    Anvil,
    Dock,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ingredient {
    pub item: ItemDefinitionId,
    pub quantity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub id: RecipeId,
    pub display_name: String,
    pub output_item: ItemDefinitionId,
    pub output_quantity: u32,
    pub ingredients: Vec<Ingredient>,
    pub required_station: StationKind,
    pub craft_time_secs: u32,
}

impl Recipe {
    pub fn total_ingredient_slots(&self) -> usize {
        self.ingredients.len()
    }
}
