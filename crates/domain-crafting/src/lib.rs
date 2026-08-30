//! domain-crafting: receitas e validação pura.

pub mod recipe;
pub mod validate;

pub use recipe::{Ingredient, Recipe, StationKind};
pub use validate::{can_craft, CraftError, InventoryView};
